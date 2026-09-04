use super::StageHandler;
use crate::LearningCycleOrchestrator;
use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

pub struct VerifyingHandler {
    #[allow(dead_code)]
    min_confidence: f32,
}

impl VerifyingHandler {
    pub fn new(min_confidence: f32) -> Self {
        Self { min_confidence }
    }
}

#[async_trait]
impl StageHandler for VerifyingHandler {
    async fn execute(
        &self,
        orchestrator: &LearningCycleOrchestrator,
        cycle_id: Uuid,
    ) -> Result<()> {
        tracing::info!("Processing VERIFYING stage for cycle {}", cycle_id);

        let evidence = orchestrator.get_evidence(cycle_id).await?;

        if evidence.is_empty() {
            return orchestrator
                .fail_cycle(cycle_id, "NO_SOURCES_FOUND")
                .await
                .map(|_| ());
        }

        // Compute confidence based on evidence quality
        // Higher trust tiers contribute more to confidence
        let total_trust: i32 = evidence.iter().map(|e| e.trust_tier).sum();
        let count = evidence.len() as f32;
        let avg_trust = total_trust as f32 / count;

        // Normalize to 0-1 range (trust tiers are 1-5)
        let trust_score = (avg_trust / 5.0).min(1.0);

        // Bonus for multiple independent sources
        let diversity_bonus = (count / 3.0).min(0.2);

        let confidence = (trust_score + diversity_bonus).min(1.0);

        // Use LLM to cross-verify findings if available
        let verify_prompt = format!(
            "Verify the following research findings for consistency and correctness.\n\n\
             Evidence count: {}\n\
             Trust scores: {}\n\
             \nRate the overall reliability from 0.0 to 1.0 and explain why.",
            evidence.len(),
            evidence
                .iter()
                .map(|e| format!("{}(tier={})", e.source_type, e.trust_tier))
                .collect::<Vec<_>>()
                .join(", ")
        );

        let llm_confidence = match orchestrator.run_inference(&verify_prompt).await {
            Ok(response) => {
                // Try to extract a numeric confidence from the response
                let extracted = response
                    .lines()
                    .find_map(|line| {
                        line.parse::<f32>().ok().or_else(|| {
                            line.split_whitespace().find_map(|word| {
                                word.trim_matches(|c: char| !c.is_ascii_digit() && c != '.')
                                    .parse::<f32>()
                                    .ok()
                            })
                        })
                    })
                    .unwrap_or(confidence);

                // Average LLM confidence with trust-based confidence
                (confidence + extracted) / 2.0
            }
            Err(e) => {
                tracing::warn!("LLM verification unavailable: {}", e);
                confidence
            }
        };

        orchestrator
            .update_confidence(cycle_id, llm_confidence)
            .await?;

        tracing::info!(
            "Verification for cycle {}: confidence={:.3} (min={})",
            cycle_id,
            llm_confidence,
            self.min_confidence
        );

        orchestrator
            .add_test_result(
                cycle_id,
                "verifying",
                llm_confidence >= self.min_confidence,
                serde_json::json!({
                    "confidence": llm_confidence,
                    "min_required": self.min_confidence,
                    "evidence_count": evidence.len(),
                    "avg_trust_tier": avg_trust,
                }),
            )
            .await?;

        if llm_confidence < self.min_confidence {
            return orchestrator
                .fail_cycle(cycle_id, "INSUFFICIENT_CONFIDENCE")
                .await
                .map(|_| ());
        }

        orchestrator.advance_cycle(cycle_id).await?;

        Ok(())
    }

    fn stage_name(&self) -> &str {
        "verifying"
    }
}
