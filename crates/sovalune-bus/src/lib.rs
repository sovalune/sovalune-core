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
    TrainingJobRequest,
    TrainingJobResult,
    MemoryDecayTick,
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
            Subject::TrainingJobRequest => write!(f, "training.job.request"),
            Subject::TrainingJobResult => write!(f, "training.job.result"),
            Subject::MemoryDecayTick => write!(f, "memory.decay.tick"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub request_id: String,
    pub session_id: String,
    pub project_id: String,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub request_id: String,
    pub cycle_id: Option<String>,
    pub tool: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolResult {
    pub request_id: String,
    pub ok: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LearningCycleEvent {
    pub cycle_id: String,
    pub project_id: String,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub detail: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrainingJobRequest {
    pub job_id: String,
    pub cycle_id: Option<String>,
    pub job_type: String,
    pub dataset_uri: String,
    pub base_artifact_uri: Option<String>,
    pub limits: TrainingLimits,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrainingLimits {
    pub max_steps: u32,
    pub timeout_seconds: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrainingJobResult {
    pub job_id: String,
    pub ok: bool,
    pub artifact_uri: Option<String>,
    pub metrics: serde_json::Value,
    pub error: Option<String>,
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

    pub async fn publish_inference_request(
        &self,
        request: &InferenceRequest,
    ) -> anyhow::Result<()> {
        let subject = format!("inference.request.{}", request.project_id);
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
        cycle_id: Option<&str>,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let subject = format!("tools.call.{}", tool_name);
        let payload = ToolCall {
            request_id: request_id.to_string(),
            cycle_id: cycle_id.map(|s| s.to_string()),
            tool: tool_name.to_string(),
            arguments: arguments.clone(),
        };
        self.client
            .publish(subject, serde_json::to_vec(&payload)?.into())
            .await?;
        Ok(())
    }

    pub async fn publish_tool_result(
        &self,
        request_id: &str,
        ok: bool,
        result: Option<serde_json::Value>,
        error: Option<&str>,
    ) -> anyhow::Result<()> {
        let subject = format!("tools.result.{}", request_id);
        let payload = ToolResult {
            request_id: request_id.to_string(),
            ok,
            result,
            error: error.map(|s| s.to_string()),
        };
        self.client
            .publish(subject, serde_json::to_vec(&payload)?.into())
            .await?;
        Ok(())
    }

    pub async fn wait_for_tool_result(
        &self,
        request_id: &str,
        timeout_ms: u64,
    ) -> anyhow::Result<ToolResult> {
        let subject = format!("tools.result.{}", request_id);
        let mut sub = self.client.subscribe(subject).await?;

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), sub.next()).await;

        match result {
            Ok(Some(msg)) => {
                let payload: ToolResult = serde_json::from_slice(&msg.payload)?;
                Ok(payload)
            }
            Ok(None) => Err(anyhow::anyhow!("Subscription ended")),
            Err(_) => Err(anyhow::anyhow!("Timeout waiting for tool result")),
        }
    }

    pub async fn publish_learning_cycle_event(
        &self,
        subject: &str,
        event: &LearningCycleEvent,
    ) -> anyhow::Result<()> {
        let full_subject = format!("{}.{}", subject, event.project_id);
        let payload = serde_json::to_vec(event)?;
        self.client.publish(full_subject, payload.into()).await?;
        Ok(())
    }

    pub async fn publish_training_job_request(
        &self,
        request: &TrainingJobRequest,
    ) -> anyhow::Result<()> {
        let payload = serde_json::to_vec(request)?;
        self.client
            .publish("training.job.request", payload.into())
            .await?;
        Ok(())
    }

    pub async fn subscribe_training_job_result<F>(&self, mut callback: F) -> anyhow::Result<()>
    where
        F: FnMut(TrainingJobResult) + Send + 'static,
    {
        let mut sub = self
            .client
            .subscribe("training.job.result".to_string())
            .await?;

        tokio::spawn(async move {
            while let Some(msg) = sub.next().await {
                if let Ok(result) = serde_json::from_slice::<TrainingJobResult>(&msg.payload) {
                    callback(result);
                }
            }
        });

        Ok(())
    }

    pub async fn publish_memory_decay_tick(&self, project_id: Option<&str>) -> anyhow::Result<()> {
        let payload = serde_json::json!({
            "project_id": project_id,
        });
        self.client
            .publish("memory.decay.tick", serde_json::to_vec(&payload)?.into())
            .await?;
        Ok(())
    }
}
