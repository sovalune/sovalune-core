use crate::{Evidence, ScoredMemory};
use sovalune_storage_client::{
    CreateMemoryEntry, MemoryEntry, MemoryFilter, MemoryRepository, MemoryTier,
};
use sqlx::PgPool;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Clone)]
pub struct VectorMemoryStore {
    repo: MemoryRepository,
}

impl VectorMemoryStore {
    pub fn new(pool: PgPool) -> Self {
        Self {
            repo: MemoryRepository::new(pool),
        }
    }

    pub async fn insert_raw(
        &self,
        project_id: Uuid,
        content: &str,
        embedding: &[f32],
        metadata: serde_json::Value,
    ) -> anyhow::Result<Uuid> {
        let entry = CreateMemoryEntry {
            project_id,
            tier: MemoryTier::Raw,
            content: content.to_string(),
            embedding: Some(embedding.to_vec()),
            metadata,
            confidence_score: Some(0.5),
            source_entry_ids: None,
        };

        let created = self.repo.create(entry).await?;
        info!("Inserted raw memory: {}", created.id);
        Ok(created.id)
    }

    pub async fn search(
        &self,
        query_embedding: &[f32],
        filter: MemoryFilter,
        top_k: usize,
    ) -> anyhow::Result<Vec<ScoredMemory>> {
        let results = self
            .repo
            .search_by_embedding(query_embedding, filter, top_k)
            .await?;

        let scored: Vec<ScoredMemory> = results
            .into_iter()
            .map(|(entry, score)| ScoredMemory { entry, score })
            .collect();

        Ok(scored)
    }

    pub async fn search_by_text(
        &self,
        query: &str,
        filter: MemoryFilter,
        limit: i64,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let filter_with_query = MemoryFilter {
            query: Some(query.to_string()),
            ..filter
        };

        self.repo.list(filter_with_query, limit, 0).await
    }

    pub async fn get_entry(&self, id: Uuid) -> anyhow::Result<Option<MemoryEntry>> {
        self.repo.get(id).await
    }

    pub async fn update_entry(
        &self,
        id: Uuid,
        content: Option<String>,
        metadata: Option<serde_json::Value>,
        confidence_score: Option<f32>,
        archived: Option<bool>,
    ) -> anyhow::Result<Option<MemoryEntry>> {
        let update = sovalune_storage_client::UpdateMemoryEntry {
            content,
            metadata,
            confidence_score,
            archived,
        };

        self.repo.update(id, update).await
    }

    pub async fn consolidate(
        &self,
        source_ids: &[Uuid],
        embedding: &[f32],
    ) -> anyhow::Result<Uuid> {
        if source_ids.is_empty() {
            return Err(anyhow::anyhow!("No source entries to consolidate"));
        }

        let mut sources = Vec::new();
        for id in source_ids {
            if let Some(entry) = self.repo.get(*id).await? {
                sources.push(entry);
            }
        }

        if sources.is_empty() {
            return Err(anyhow::anyhow!("No valid source entries found"));
        }

        let contents: Vec<&str> = sources.iter().map(|s| s.content.as_str()).collect();
        let consolidated_content = format!(
            "Consolidated from {} sources:\n{}",
            sources.len(),
            contents.join("\n---\n")
        );

        let metadata = serde_json::json!({
            "source_count": sources.len(),
            "source_ids": source_ids,
        });

        let entry = self
            .repo
            .consolidate(source_ids, &consolidated_content, embedding, metadata)
            .await?;
        info!(
            "Consolidated {} sources into {}",
            source_ids.len(),
            entry.id
        );

        Ok(entry.id)
    }

    pub async fn promote_to_verified(&self, id: Uuid, _evidence: Evidence) -> anyhow::Result<()> {
        self.repo.promote_to_verified(id, Uuid::new_v4()).await?;
        info!("Promoted memory {} to verified", id);
        Ok(())
    }

    pub async fn decay_tick(&self) -> anyhow::Result<usize> {
        let affected = self.repo.decay_tick().await?;
        info!("Decay tick affected {} entries", affected);
        Ok(affected)
    }

    pub async fn archive_low_decay(&self, threshold: f32) -> anyhow::Result<usize> {
        let affected = self.repo.archive_low_decay(threshold).await?;
        if affected > 0 {
            warn!(
                "Archived {} entries with decay below {}",
                affected, threshold
            );
        }
        Ok(affected)
    }

    pub async fn delete(&self, id: Uuid) -> anyhow::Result<()> {
        self.repo.delete(id).await?;
        info!("Deleted memory entry: {}", id);
        Ok(())
    }
}
