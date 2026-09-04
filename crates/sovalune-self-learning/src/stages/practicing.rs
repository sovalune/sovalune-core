use super::StageHandler;
use crate::LearningCycleOrchestrator;
use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

pub struct PracticingHandler;

#[async_trait]
impl StageHandler for PracticingHandler {
    async fn execute(
        &self,
        orchestrator: &LearningCycleOrchestrator,
        cycle_id: Uuid,
    ) -> Result<()> {
        tracing::info!("Processing PRACTICING stage for cycle {}", cycle_id);

        let cycle = orchestrator.get_cycle(cycle_id).await?;
        let evidence = orchestrator.get_evidence(cycle_id).await?;

        // Generate synthetic practice tasks based on research
        let evidence_summary: Vec<&str> = evidence.iter().map(|e| e.excerpt.as_str()).collect();

        let practice_prompt = format!(
            "Based on the research findings below, generate 3 synthetic practice tasks \
             that test the knowledge gained.\n\n\
             Research findings:\n{}\n\n\
             Return a JSON array of tasks: [{{\"task\": \"...\", \"expected_approach\": \"...\", \"difficulty\": \"easy|medium|hard\"}}]",
            evidence_summary.join("\n---\n")
        );

        match orchestrator.run_inference(&practice_prompt).await {
            Ok(practice_tasks) => {
                orchestrator
                    .add_test_result(
                        cycle_id,
                        "practicing",
                        true,
                        serde_json::json!({
                            "practice_tasks": practice_tasks,
                            "evidence_count": evidence.len(),
                            "origin_task_id": cycle.origin_task_id.to_string(),
                        }),
                    )
                    .await?;

                tracing::info!(
                    "Generated practice tasks for cycle {}: {}",
                    cycle_id,
                    &practice_tasks[..200.min(practice_tasks.len())]
                );
            }
            Err(e) => {
                tracing::warn!("Failed to generate practice tasks: {}", e);
                // Record that practice was limited
                orchestrator
                    .add_test_result(
                        cycle_id,
                        "practicing",
                        false,
                        serde_json::json!({
                            "error": e.to_string(),
                            "fallback": true,
                        }),
                    )
                    .await?;

                // Still advance - practice is optional if inference is unavailable
                tracing::info!("Advancing despite practice failure (inference unavailable)");
            }
        }

        orchestrator.advance_cycle(cycle_id).await?;

        Ok(())
    }

    fn stage_name(&self) -> &str {
        "practicing"
    }
}
