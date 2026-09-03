use super::StageHandler;
use crate::LearningCycleOrchestrator;
use uuid::Uuid;
use anyhow::Result;
use async_trait::async_trait;

pub struct VerifyingHandler {
    min_confidence: f32,
}

impl VerifyingHandler {
    pub fn new(min_confidence: f32) -> Self {
        Self { min_confidence }
    }
}

#[async_trait]
impl StageHandler for VerifyingHandler {
    async fn execute(&self, orchestrator: &LearningCycleOrchestrator, cycle_id: Uuid) -> Result<()> {
        tracing::info!("Processing VERIFYING stage for cycle {}", cycle_id);
        
        let evidence = orchestrator.get_evidence(cycle_id).await?;
        
        if evidence.is_empty() {
            return orchestrator.fail_cycle(cycle_id, "NO_SOURCES_FOUND").await.map(|_| ());
        }
        
        let confidence = 0.8;
        orchestrator.update_confidence(cycle_id, confidence).await?;
        
        if confidence < self.min_confidence {
            return orchestrator.fail_cycle(cycle_id, "INSUFFICIENT_CONFIDENCE").await.map(|_| ());
        }
        
        orchestrator.advance_cycle(cycle_id).await?;
        
        Ok(())
    }
    
    fn stage_name(&self) -> &str {
        "verifying"
    }
}
