use crate::{MemoryFilter, ScoredMemory, VectorMemoryStore};
use sqlx::PgPool;

pub struct ContextWeaver {
    pool: PgPool,
    max_memory_tokens: usize,
    total_context_window: usize,
}

impl ContextWeaver {
    pub fn new(pool: PgPool, max_memory_tokens: usize, total_context_window: usize) -> Self {
        Self {
            pool,
            max_memory_tokens,
            total_context_window,
        }
    }

    pub async fn build_context(
        &self,
        query: &str,
        query_embedding: &[f32],
        project_id: uuid::Uuid,
        history: &[(String, String)],
    ) -> anyhow::Result<String> {
        let store = VectorMemoryStore::new(self.pool.clone());

        let filter = MemoryFilter {
            project_id: Some(project_id),
            tier: None,
            min_confidence: Some(0.5),
            archived: Some(false),
        };

        let memories = store.search(query_embedding, filter, 10).await?;

        let mut context = String::new();

        // Add verified facts
        let verified: Vec<&ScoredMemory> = memories
            .iter()
            .filter(|m| m.entry.tier == crate::MemoryTier::Verified)
            .collect();

        if !verified.is_empty() {
            context.push_str("<verified_facts>\n");
            for mem in verified {
                context.push_str(&format!("- {}\n", mem.entry.content));
            }
            context.push_str("</verified_facts>\n\n");
        }

        // Add consolidated knowledge
        let consolidated: Vec<&ScoredMemory> = memories
            .iter()
            .filter(|m| m.entry.tier == crate::MemoryTier::Consolidated)
            .collect();

        if !consolidated.is_empty() {
            context.push_str("<project_conventions>\n");
            for mem in consolidated {
                context.push_str(&format!("- {}\n", mem.entry.content));
            }
            context.push_str("</project_conventions>\n\n");
        }

        // Add recent context from history
        if !history.is_empty() {
            context.push_str("<recent_context>\n");
            for (role, content) in history.iter().rev().take(5) {
                context.push_str(&format!("{}: {}\n", role, content));
            }
            context.push_str("</recent_context>\n");
        }

        Ok(context)
    }
}
