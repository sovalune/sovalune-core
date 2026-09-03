use super::StageHandler;
use crate::LearningCycleOrchestrator;
use uuid::Uuid;
use anyhow::Result;
use async_trait::async_trait;

pub struct DetectedHandler;

#[async_trait]
impl StageHandler for DetectedHandler {
    async fn execute(&self, orchestrator: &LearningCycleOrchestrator, cycle_id: Uuid) -> Result<()> {
        tracing::info!("Processing DETECTED stage for cycle {}", cycle_id);
        
        orchestrator.advance_cycle(cycle_id).await?;
        
        Ok(())
    }
    
    fn stage_name(&self) -> &str {
        "detected"
    }
}
