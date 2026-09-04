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
        let evidence = orchestrator.get_evidence(cycle_id).await?;

        // Apply the learned knowledge back to the project
        let evidence_summary: Vec<&str> = evidence.iter().map(|e| e.excerpt.as_str()).collect();

        let apply_prompt = format!(
            "Apply the following learned knowledge to improve the project.\n\n\
             Task ID: {}\n\
             Project: {}\n\
             Confidence: {:?}\n\
             Knowledge gathered:\n{}\n\n\
             Generate an actionable summary:\n\
             1. What specific changes should be made\n\
             2. What files or modules are affected\n\
             3. What conventions or patterns should be followed\n\n\
             Return a JSON object: {{\"summary\": \"...\", \"changes\": [\"...\"], \"files_affected\": [\"...\"]}}",
            cycle.origin_task_id,
            cycle.project_id,
            cycle.confidence_score,
            evidence_summary.join("\n---\n")
        );

        match orchestrator.run_inference(&apply_prompt).await {
            Ok(application) => {
                orchestrator
                    .add_test_result(
                        cycle_id,
                        "applying",
                        true,
                        serde_json::json!({
                            "application": application,
                            "origin_task_id": cycle.origin_task_id.to_string(),
                            "project_id": cycle.project_id.to_string(),
                        }),
                    )
                    .await?;

                // Store the applied knowledge in vector memory for future reference
                if let Some(ref store) = orchestrator.vector_memory {
                    let knowledge_content = format!(
                        "Learning from task {} (project {}):\n{}",
                        cycle.origin_task_id,
                        cycle.project_id,
                        &application[..500.min(application.len())]
                    );

                    if let Err(e) = store
                        .insert_raw_with_embedding(
                            cycle.project_id,
                            &knowledge_content,
                            serde_json::json!({
                                "source": "self_learning",
                                "cycle_id": cycle_id.to_string(),
                                "origin_task_id": cycle.origin_task_id.to_string(),
                            }),
                        )
                        .await
                    {
                        tracing::warn!("Failed to store applied knowledge: {}", e);
                    }
                }

                tracing::info!(
                    "Applied knowledge for cycle {}: {}",
                    cycle_id,
                    &application[..200.min(application.len())]
                );
            }
            Err(e) => {
                tracing::warn!("Failed to apply knowledge: {}", e);
                orchestrator
                    .add_test_result(
                        cycle_id,
                        "applying",
                        false,
                        serde_json::json!({
                            "error": e.to_string(),
                            "fallback": true,
                        }),
                    )
                    .await?;
            }
        }

        orchestrator.advance_cycle(cycle_id).await?;

        Ok(())
    }

    fn stage_name(&self) -> &str {
        "applying"
    }
}
