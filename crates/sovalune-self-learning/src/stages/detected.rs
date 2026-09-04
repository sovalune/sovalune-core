use super::StageHandler;
use crate::LearningCycleOrchestrator;
use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

pub struct DetectedHandler;

#[async_trait]
impl StageHandler for DetectedHandler {
    async fn execute(
        &self,
        orchestrator: &LearningCycleOrchestrator,
        cycle_id: Uuid,
    ) -> Result<()> {
        tracing::info!("Processing DETECTED stage for cycle {}", cycle_id);

        let cycle = orchestrator.get_cycle(cycle_id).await?;

        // Analyze the failure to understand what happened
        let analysis_prompt = format!(
            "A task with ID {} failed. Analyze what might have gone wrong and suggest a research plan. \
             Return a JSON object with analysis and research_queries fields.",
            cycle.origin_task_id
        );

        let analysis = match orchestrator.run_inference(&analysis_prompt).await {
            Ok(response) => {
                orchestrator
                    .add_evidence(
                        cycle_id,
                        "analysis",
                        None,
                        &response,
                        2, // System-generated analysis
                    )
                    .await?;
                response
            }
            Err(e) => {
                tracing::warn!("Failed to run analysis inference: {}", e);
                "Analysis not available - inference engine not configured".to_string()
            }
        };

        tracing::info!("Detected analysis for cycle {}: {}", cycle_id, &analysis[..200.min(analysis.len())]);

        orchestrator.advance_cycle(cycle_id).await?;

        Ok(())
    }

    fn stage_name(&self) -> &str {
        "detected"
    }
}
