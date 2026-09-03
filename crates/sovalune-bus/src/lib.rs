use async_nats::Client;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use tracing::info;

#[derive(Debug, Clone)]
pub enum Subject {
    InferenceRequest,
    InferenceResponse,
    ToolCall,
    ToolResult,
    LearningCycleStarted,
    LearningCycleStageCompleted,
    LearningCycleFinished,
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Subject::InferenceRequest => write!(f, "inference.request"),
            Subject::InferenceResponse => write!(f, "inference.response"),
            Subject::ToolCall => write!(f, "tools.call"),
            Subject::ToolResult => write!(f, "tools.result"),
            Subject::LearningCycleStarted => write!(f, "learning.cycle.started"),
            Subject::LearningCycleStageCompleted => write!(f, "learning.cycle.stage_changed"),
            Subject::LearningCycleFinished => write!(f, "learning.cycle.finished"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub request_id: String,
    pub session_id: String,
    pub prompt_context: PromptContext,
    pub generation_config: GenerationConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PromptContext {
    pub system: String,
    pub memory_sections: Vec<MemorySection>,
    pub history: Vec<HistoryEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemorySection {
    pub tier: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GenerationConfig {
    pub max_tokens: u32,
    pub temperature: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InferenceResponse {
    pub request_id: String,
    #[serde(flatten)]
    pub payload: InferencePayload,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InferencePayload {
    Delta { delta: String },
    Done { done: bool, message_id: String },
}

#[derive(Clone)]
pub struct NatsClient {
    client: Client,
}

impl NatsClient {
    pub async fn new(nats_url: &str) -> anyhow::Result<Self> {
        let client = async_nats::connect(nats_url).await?;
        info!("Connected to NATS at {}", nats_url);
        Ok(Self { client })
    }
    
    pub fn client(&self) -> &Client {
        &self.client
    }
    
    pub async fn health_check(&self) -> bool {
        self.client.connection_state() == async_nats::connection::State::Connected
    }
    
    pub async fn publish_inference_request(&self, request: &InferenceRequest) -> anyhow::Result<()> {
        let subject = format!("inference.request.{}", request.session_id);
        let payload = serde_json::to_vec(request)?;
        self.client.publish(subject, payload.into()).await?;
        Ok(())
    }
    
    pub async fn subscribe_inference_response<F>(
        &self,
        request_id: &str,
        mut callback: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut(InferenceResponse) + Send + 'static,
    {
        let subject = format!("inference.response.{}", request_id);
        let mut sub = self.client.subscribe(subject).await?;
        
        tokio::spawn(async move {
            while let Some(msg) = sub.next().await {
                if let Ok(response) = serde_json::from_slice::<InferenceResponse>(&msg.payload) {
                    callback(response);
                }
            }
        });
        
        Ok(())
    }
    
    pub async fn publish_tool_call(
        &self,
        request_id: &str,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let subject = format!("tools.call.{}", tool_name);
        let payload = serde_json::json!({
            "request_id": request_id,
            "tool": tool_name,
            "arguments": arguments,
        });
        self.client.publish(subject, serde_json::to_vec(&payload)?.into()).await?;
        Ok(())
    }
    
    pub async fn wait_for_tool_result(&self, request_id: &str, timeout_ms: u64) -> anyhow::Result<serde_json::Value> {
        let subject = format!("tools.result.{}", request_id);
        let mut sub = self.client.subscribe(subject).await?;
        
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            sub.next()
        ).await;
        
        match result {
            Ok(Some(msg)) => {
                let payload: serde_json::Value = serde_json::from_slice(&msg.payload)?;
                Ok(payload)
            }
            Ok(None) => Err(anyhow::anyhow!("Subscription ended")),
            Err(_) => Err(anyhow::anyhow!("Timeout waiting for tool result")),
        }
    }
}
