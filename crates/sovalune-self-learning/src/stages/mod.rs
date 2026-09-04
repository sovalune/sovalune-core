pub mod applying;
pub mod detected;
pub mod practicing;
pub mod researching;
pub mod testing;
pub mod verifying;

use crate::LearningCycleOrchestrator;
use anyhow::Result;
use uuid::Uuid;

#[async_trait::async_trait]
pub trait StageHandler: Send + Sync {
    async fn execute(&self, orchestrator: &LearningCycleOrchestrator, cycle_id: Uuid)
        -> Result<()>;
    fn stage_name(&self) -> &str;
}
