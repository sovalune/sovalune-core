use crate::{ScoredMemory, VectorMemoryStore};
use sovalune_storage_client::{MemoryFilter, MemoryTier};
use uuid::Uuid;

pub struct ContextWeaver {
    store: VectorMemoryStore,
    max_memory_tokens: usize,
    #[allow(dead_code)]
    total_context_window: usize,
    recency_weight: f32,
    confidence_weight: f32,
}

impl ContextWeaver {
    pub fn new(
        store: VectorMemoryStore,
        max_memory_tokens: usize,
        total_context_window: usize,
    ) -> Self {
        Self {
            store,
            max_memory_tokens,
            total_context_window,
            recency_weight: 0.3,
            confidence_weight: 0.7,
        }
    }

    pub async fn build_context(
        &self,
        _query: &str,
        query_embedding: &[f32],
        project_id: Uuid,
        history: &[(String, String)],
        include_raw: bool,
    ) -> anyhow::Result<String> {
        let mut context = String::new();
        let mut token_budget = self.max_memory_tokens;

        let verified_filter = MemoryFilter {
            project_id: Some(project_id),
            tier: Some(MemoryTier::Verified),
            min_confidence: Some(0.8),
            archived: Some(false),
            query: None,
        };

        let verified = self
            .store
            .search(query_embedding, verified_filter, 10)
            .await?;
        let verified_tokens =
            self.add_section(&mut context, "<verified_facts>", &verified, token_budget);
        token_budget = token_budget.saturating_sub(verified_tokens);

        let consolidated_filter = MemoryFilter {
            project_id: Some(project_id),
            tier: Some(MemoryTier::Consolidated),
            min_confidence: Some(0.6),
            archived: Some(false),
            query: None,
        };

        let consolidated = self
            .store
            .search(query_embedding, consolidated_filter, 10)
            .await?;
        let consolidated_tokens = self.add_section(
            &mut context,
            "<project_conventions>",
            &consolidated,
            token_budget,
        );
        token_budget = token_budget.saturating_sub(consolidated_tokens);

        if include_raw && token_budget > 100 {
            let raw_filter = MemoryFilter {
                project_id: Some(project_id),
                tier: Some(MemoryTier::Raw),
                min_confidence: Some(0.4),
                archived: Some(false),
                query: None,
            };

            let raw = self.store.search(query_embedding, raw_filter, 5).await?;
            self.add_section(&mut context, "<recent_discoveries>", &raw, token_budget);
        }

        if !history.is_empty() {
            context.push_str("\n<recent_context>\n");
            for (role, content) in history.iter().rev().take(5) {
                context.push_str(&format!("{}: {}\n", role, content));
            }
            context.push_str("</recent_context>\n");
        }

        Ok(context)
    }

    fn add_section(
        &self,
        context: &mut String,
        tag: &str,
        memories: &[ScoredMemory],
        budget: usize,
    ) -> usize {
        if memories.is_empty() {
            return 0;
        }

        context.push_str(&format!("\n{}\n", tag));

        let mut tokens_used = 0;
        let token_per_char = 4;

        for memory in memories {
            let estimated_tokens = memory.entry.content.len() / token_per_char;

            if tokens_used + estimated_tokens > budget {
                let remaining_chars = (budget - tokens_used) * token_per_char;
                if remaining_chars > 50 {
                    let truncated =
                        &memory.entry.content[..remaining_chars.min(memory.entry.content.len())];
                    context.push_str(&format!("- {}...\n", truncated));
                }
                break;
            }

            context.push_str(&format!("- {}\n", memory.entry.content));
            tokens_used += estimated_tokens;
        }

        context.push_str(&format!(
            "</{}>\n",
            tag.trim_start_matches('<').trim_end_matches('>')
        ));

        tokens_used
    }

    pub fn set_weights(&mut self, recency_weight: f32, confidence_weight: f32) {
        self.recency_weight = recency_weight;
        self.confidence_weight = confidence_weight;
    }
}
