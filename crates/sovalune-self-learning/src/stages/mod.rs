pub mod detected;
pub mod researching;
pub mod verifying;
pub mod practicing;
pub mod testing;
pub mod applying;

use crate::LearningCycleOrchestrator;
use uuid::Uuid;
use anyhow::Result;

#[async_trait::async_trait]
pub trait StageHandler: Send + Sync {
    async fn execute(&self, orchestrator: &LearningCycleOrchestrator, cycle_id: Uuid) -> Result<()>;
    fn stage_name(&self) -> &str;
}
