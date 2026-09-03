use super::StageHandler;
use crate::LearningCycleOrchestrator;
use uuid::Uuid;
use anyhow::Result;
use async_trait::async_trait;

pub struct ResearchingHandler {
    max_sources: usize,
    timeout_seconds: u64,
}

impl ResearchingHandler {
    pub fn new(max_sources: usize, timeout_seconds: u64) -> Self {
        Self {
            max_sources,
            timeout_seconds,
        }
    }
}

#[async_trait]
impl StageHandler for ResearchingHandler {
    async fn execute(&self, orchestrator: &LearningCycleOrchestrator, cycle_id: Uuid) -> Result<()> {
        tracing::info!("Processing RESEARCHING stage for cycle {}", cycle_id);
        
        let cycle = orchestrator.get_cycle(cycle_id).await?;
        
        orchestrator.add_evidence(
            cycle_id,
            "system",
            None,
            &format!("Research started for task {}", cycle.origin_task_id),
            1,
        ).await?;
        
        orchestrator.advance_cycle(cycle_id).await?;
        
        Ok(())
    }
    
    fn stage_name(&self) -> &str {
        "researching"
    }
}
