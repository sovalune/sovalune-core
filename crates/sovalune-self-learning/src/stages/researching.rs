use super::StageHandler;
use crate::LearningCycleOrchestrator;
use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

pub struct ResearchingHandler {
    #[allow(dead_code)]
    max_sources: usize,
    #[allow(dead_code)]
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
    async fn execute(
        &self,
        orchestrator: &LearningCycleOrchestrator,
        cycle_id: Uuid,
    ) -> Result<()> {
        tracing::info!("Processing RESEARCHING stage for cycle {}", cycle_id);

        let cycle = orchestrator.get_cycle(cycle_id).await?;

        // Step 1: Search vector memory for relevant context
        let memory_context = match orchestrator
            .search_memory(
                &format!("task {} failure knowledge", cycle.origin_task_id),
                self.max_sources,
            )
            .await
        {
            Ok(results) => {
                if !results.is_empty() {
                    let context = results.join("\n---\n");
                    orchestrator
                        .add_evidence(
                            cycle_id,
                            "vector_memory",
                            None,
                            &context,
                            3, // Consolidated knowledge
                        )
                        .await?;
                    Some(context)
                } else {
                    None
                }
            }
            Err(e) => {
                tracing::warn!("Failed to search memory: {}", e);
                None
            }
        };

        // Step 2: Use inference to generate research findings
        let research_prompt = format!(
            "Research the following task failure and gather insights.\n\n\
             Task ID: {}\n\
             Project: {}\n\
             {}\
             \n\nBased on available information, provide:\n\
             1. What likely went wrong\n\
             2. Relevant patterns or conventions\n\
             3. Recommended fixes or approaches\n\
             Return a JSON object with findings, confidence (0.0-1.0), and sources array.",
            cycle.origin_task_id,
            cycle.project_id,
            memory_context
                .map(|c| format!("Existing knowledge:\n{}", c))
                .unwrap_or_else(|| "No existing knowledge found.".to_string())
        );

        match orchestrator.run_inference(&research_prompt).await {
            Ok(findings) => {
                orchestrator
                    .add_evidence(
                        cycle_id,
                        "llm_research",
                        None,
                        &findings,
                        4, // High trust - LLM analysis
                    )
                    .await?;
                tracing::info!(
                    "Research findings for cycle {}: {}",
                    cycle_id,
                    &findings[..200.min(findings.len())]
                );
            }
            Err(e) => {
                tracing::warn!("Failed to run research inference: {}", e);
                orchestrator
                    .add_evidence(
                        cycle_id,
                        "system",
                        None,
                        &format!("Research limited - inference unavailable: {}", e),
                        1,
                    )
                    .await?;
            }
        }

        orchestrator.advance_cycle(cycle_id).await?;

        Ok(())
    }

    fn stage_name(&self) -> &str {
        "researching"
    }
}
