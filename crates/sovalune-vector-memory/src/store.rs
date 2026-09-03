use crate::{MemoryEntry, MemoryFilter, MemoryTier, RawMemoryEntry, ScoredMemory};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct VectorMemoryStore {
    pool: PgPool,
}

impl VectorMemoryStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert_raw(&self, entry: RawMemoryEntry) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();
        let embedding_json = serde_json::to_string(&entry.embedding)?;
        
        sqlx::query(
            r#"
            INSERT INTO memory_entries (id, project_id, tier, content, embedding, metadata)
            VALUES ($1, $2, 'raw', $3, $4::vector, $5)
            "#,
        )
        .bind(id)
        .bind(entry.project_id)
        .bind(&entry.content)
        .bind(&embedding_json)
        .bind(&entry.metadata)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    pub async fn search(
        &self,
        query_embedding: &[f32],
        filter: MemoryFilter,
        top_k: usize,
    ) -> anyhow::Result<Vec<ScoredMemory>> {
        let embedding_json = serde_json::to_string(query_embedding)?;
        
        let mut query = String::from(
            r#"
            SELECT *, 1 - (embedding <=> $1::vector) as score
            FROM memory_entries
            WHERE archived = false
            "#,
        );

        if let Some(project_id) = filter.project_id {
            query.push_str(&format!(" AND project_id = '{}'", project_id));
        }

        if let Some(tier) = filter.tier {
            query.push_str(&format!(" AND tier = '{}'", tier));
        }

        if let Some(min_confidence) = filter.min_confidence {
            query.push_str(&format!(" AND confidence_score >= {}", min_confidence));
        }

        query.push_str(" ORDER BY score DESC");
        query.push_str(&format!(" LIMIT {}", top_k));

        let rows = sqlx::query_as::<_, (Uuid, Uuid, String, String, Option<String>, serde_json::Value, f32, f32, bool, Option<Vec<Uuid>>, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, f32)>(&query)
            .bind(&embedding_json)
            .fetch_all(&self.pool)
            .await?;

        let mut results = Vec::new();
        for row in rows {
            let entry = MemoryEntry {
                id: row.0,
                project_id: row.1,
                tier: match row.2.as_str() {
                    "raw" => MemoryTier::Raw,
                    "consolidated" => MemoryTier::Consolidated,
                    "verified" => MemoryTier::Verified,
                    _ => MemoryTier::Raw,
                },
                content: row.3,
                embedding: row.4.and_then(|e| serde_json::from_str(&e).ok()),
                metadata: row.5,
                confidence_score: row.6,
                decay_score: row.7,
                archived: row.8,
                source_entry_ids: row.9,
                created_at: row.10,
                updated_at: row.11,
            };
            let score = row.12;
            results.push(ScoredMemory { entry, score });
        }

        Ok(results)
    }

    pub async fn get_entry(&self, id: Uuid) -> anyhow::Result<Option<MemoryEntry>> {
        let row = sqlx::query_as::<_, (Uuid, Uuid, String, String, Option<String>, serde_json::Value, f32, f32, bool, Option<Vec<Uuid>>, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>(
            r#"
            SELECT id, project_id, tier, content, embedding, metadata, 
                   confidence_score, decay_score, archived, source_entry_ids,
                   created_at, updated_at
            FROM memory_entries
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(MemoryEntry {
                id: row.0,
                project_id: row.1,
                tier: match row.2.as_str() {
                    "raw" => MemoryTier::Raw,
                    "consolidated" => MemoryTier::Consolidated,
                    "verified" => MemoryTier::Verified,
                    _ => MemoryTier::Raw,
                },
                content: row.3,
                embedding: row.4.and_then(|e| serde_json::from_str(&e).ok()),
                metadata: row.5,
                confidence_score: row.6,
                decay_score: row.7,
                archived: row.8,
                source_entry_ids: row.9,
                created_at: row.10,
                updated_at: row.11,
            })),
            None => Ok(None),
        }
    }

    pub async fn update_entry(&self, id: Uuid, content: Option<String>, metadata: Option<serde_json::Value>) -> anyhow::Result<()> {
        if let Some(content) = content {
            sqlx::query("UPDATE memory_entries SET content = $1, updated_at = NOW() WHERE id = $2")
                .bind(content)
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        if let Some(metadata) = metadata {
            sqlx::query("UPDATE memory_entries SET metadata = $1, updated_at = NOW() WHERE id = $2")
                .bind(metadata)
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    pub async fn archive_entry(&self, id: Uuid) -> anyhow::Result<()> {
        sqlx::query("UPDATE memory_entries SET archived = true, updated_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_entry(&self, id: Uuid) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM memory_entries WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn promote_to_verified(&self, id: Uuid, _evidence: crate::Evidence) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            UPDATE memory_entries
            SET tier = 'verified', confidence_score = 1.0, updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn decay_tick(&self) -> anyhow::Result<usize> {
        let result = sqlx::query(
            r#"
            UPDATE memory_entries
            SET decay_score = decay_score * 0.95
            WHERE tier = 'raw'
            AND archived = false
            AND id NOT IN (
                SELECT DISTINCT unnest(source_entry_ids)
                FROM memory_entries
                WHERE source_entry_ids IS NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() as usize)
    }
}
