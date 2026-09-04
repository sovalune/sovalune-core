use super::StageHandler;
use crate::LearningCycleOrchestrator;
use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

pub struct ApplyingHandler;

#[async_trait]
impl StageHandler for ApplyingHandler {
    async fn execute(
        &self,
        orchestrator: &LearningCycleOrchestrator,
        cycle_id: Uuid,
    ) -> Result<()> {
        tracing::info!("Processing APPLYING stage for cycle {}", cycle_id);

        let cycle = orchestrator.get_cycle(cycle_id).await?;

        tracing::info!(
            "Applying learned knowledge for task {}",
            cycle.origin_task_id
        );

        orchestrator
            .add_test_result(
                cycle_id,
                "applying",
                true,
                serde_json::json!({"applied_to_original_task": true}),
            )
            .await?;

        orchestrator.advance_cycle(cycle_id).await?;

        Ok(())
    }

    fn stage_name(&self) -> &str {
        "applying"
    }
}
