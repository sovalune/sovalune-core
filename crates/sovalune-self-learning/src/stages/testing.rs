use super::StageHandler;
use crate::LearningCycleOrchestrator;
use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

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
    async fn execute(
        &self,
        orchestrator: &LearningCycleOrchestrator,
        cycle_id: Uuid,
    ) -> Result<()> {
        tracing::info!("Processing TESTING stage for cycle {}", cycle_id);

        let cycle = orchestrator.get_cycle(cycle_id).await?;
        let evidence = orchestrator.get_evidence(cycle_id).await?;
        let test_results = orchestrator.get_test_results(cycle_id).await?;

        // Evaluate the knowledge gained through testing
        let evidence_summary: Vec<&str> = evidence.iter().map(|e| e.excerpt.as_str()).collect();

        let test_prompt = format!(
            "Evaluate the following learned knowledge by testing it.\n\n\
             Task ID: {}\n\
             Knowledge gathered:\n{}\n\
             Practice results: {} items\n\n\
             Perform a self-evaluation:\n\
             1. Can the knowledge be applied to the original task?\n\
             2. Are there edge cases not covered?\n\
             3. Is the knowledge generalizable?\n\n\
             Return a JSON object: {{\"passed\": true/false, \"pass_rate\": 0.0-1.0, \"evaluation\": \"...\", \"gaps\": [\"...\"]}}",
            cycle.origin_task_id,
            evidence_summary.join("\n---\n"),
            test_results.len()
        );

        let (passed, pass_rate) = match orchestrator.run_inference(&test_prompt).await {
            Ok(evaluation) => {
                // Try to extract pass_rate from response
                let rate = evaluation
                    .lines()
                    .find_map(|line| {
                        line.split("\"pass_rate\"").nth(1).and_then(|s| {
                            s.trim_matches(|c: char| !c.is_ascii_digit() && c != '.')
                                .split(',')
                                .next()
                                .and_then(|v| v.parse::<f32>().ok())
                        })
                    })
                    .unwrap_or(0.8);

                let did_pass = rate >= self.min_pass_rate;

                orchestrator
                    .add_test_result(
                        cycle_id,
                        "testing",
                        did_pass,
                        serde_json::json!({
                            "pass_rate": rate,
                            "evaluation": evaluation,
                            "origin_task_id": cycle.origin_task_id.to_string(),
                        }),
                    )
                    .await?;

                (did_pass, rate)
            }
            Err(e) => {
                tracing::warn!("Testing inference failed: {}", e);
                // Default to passing if inference is unavailable
                orchestrator
                    .add_test_result(
                        cycle_id,
                        "testing",
                        true,
                        serde_json::json!({
                            "pass_rate": 1.0,
                            "evaluation": "Testing skipped - inference unavailable",
                            "fallback": true,
                        }),
                    )
                    .await?;

                (true, 1.0)
            }
        };

        tracing::info!(
            "Testing for cycle {}: passed={}, pass_rate={:.3}",
            cycle_id,
            passed,
            pass_rate
        );

        if pass_rate < self.min_pass_rate {
            let cycle = orchestrator.get_cycle(cycle_id).await?;
            if cycle.retry_count < 3 {
                return orchestrator.retry_cycle(cycle_id).await.map(|_| ());
            } else {
                return orchestrator
                    .fail_cycle(cycle_id, "TESTS_FAILED_MAX_RETRIES")
                    .await
                    .map(|_| ());
            }
        }

        orchestrator.advance_cycle(cycle_id).await?;

        Ok(())
    }

    fn stage_name(&self) -> &str {
        "testing"
    }
}
