use async_nats::Client;
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
        self.client.connection_info().await.is_ok()
    }
}
