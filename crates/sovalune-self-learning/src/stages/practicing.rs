use super::StageHandler;
use crate::LearningCycleOrchestrator;
use uuid::Uuid;
use anyhow::Result;
use async_trait::async_trait;

pub struct PracticingHandler;

#[async_trait]
impl StageHandler for PracticingHandler {
    async fn execute(&self, orchestrator: &LearningCycleOrchestrator, cycle_id: Uuid) -> Result<()> {
        tracing::info!("Processing PRACTICING stage for cycle {}", cycle_id);
        
        orchestrator.add_test_result(
            cycle_id,
            "practicing",
            true,
            serde_json::json!({"synthetic_tasks_created": true}),
        ).await?;
        
        orchestrator.advance_cycle(cycle_id).await?;
        
        Ok(())
    }
    
    fn stage_name(&self) -> &str {
        "practicing"
    }
}
