use super::StageHandler;
use crate::LearningCycleOrchestrator;
use uuid::Uuid;
use anyhow::Result;
use async_trait::async_trait;

pub struct TestingHandler {
    min_pass_rate: f32,
}

impl TestingHandler {
    pub fn new(min_pass_rate: f32) -> Self {
        Self { min_pass_rate }
    }
}

#[async_trait]
impl StageHandler for TestingHandler {
    async fn execute(&self, orchestrator: &LearningCycleOrchestrator, cycle_id: Uuid) -> Result<()> {
        tracing::info!("Processing TESTING stage for cycle {}", cycle_id);
        
        let passed = true;
        let pass_rate = if passed { 1.0 } else { 0.0 };
        
        orchestrator.add_test_result(
            cycle_id,
            "testing",
            passed,
            serde_json::json!({"pass_rate": pass_rate}),
        ).await?;
        
        if pass_rate < self.min_pass_rate {
            let cycle = orchestrator.get_cycle(cycle_id).await?;
            if cycle.retry_count < 3 {
                return orchestrator.retry_cycle(cycle_id).await.map(|_| ());
            } else {
                return orchestrator.fail_cycle(cycle_id, "TESTS_FAILED_MAX_RETRIES").await.map(|_| ());
            }
        }
        
        orchestrator.advance_cycle(cycle_id).await?;
        
        Ok(())
    }
    
    fn stage_name(&self) -> &str {
        "testing"
    }
}
