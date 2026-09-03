use sqlx::PgPool;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryTier {
    Raw,
    Consolidated,
    Verified,
}

impl std::fmt::Display for MemoryTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryTier::Raw => write!(f, "raw"),
            MemoryTier::Consolidated => write!(f, "consolidated"),
            MemoryTier::Verified => write!(f, "verified"),
        }
    }
}

impl std::str::FromStr for MemoryTier {
    type Err = anyhow::Error;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "raw" => Ok(MemoryTier::Raw),
            "consolidated" => Ok(MemoryTier::Consolidated),
            "verified" => Ok(MemoryTier::Verified),
            _ => Err(anyhow::anyhow!("Invalid tier: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MemoryEntry {
    pub id: Uuid,
    pub project_id: Uuid,
    pub tier: String,
    pub content: String,
    pub embedding: Option<String>,
    pub metadata: serde_json::Value,
    pub confidence_score: f32,
    pub decay_score: f32,
    pub archived: bool,
    pub source_entry_ids: Option<Vec<Uuid>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SearchMemoryRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub tier: String,
    pub content: String,
    pub embedding: Option<String>,
    pub metadata: serde_json::Value,
    pub confidence_score: f32,
    pub decay_score: f32,
    pub archived: bool,
    pub source_entry_ids: Option<Vec<Uuid>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMemoryEntry {
    pub project_id: Uuid,
    pub tier: MemoryTier,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub metadata: serde_json::Value,
    pub confidence_score: Option<f32>,
    pub source_entry_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMemoryEntry {
    pub content: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub confidence_score: Option<f32>,
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFilter {
    pub project_id: Option<Uuid>,
    pub tier: Option<MemoryTier>,
    pub min_confidence: Option<f32>,
    pub archived: Option<bool>,
    pub query: Option<String>,
}

#[derive(Clone)]
pub struct MemoryRepository {
    pool: PgPool,
}

impl MemoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create(&self, entry: CreateMemoryEntry) -> anyhow::Result<MemoryEntry> {
        let id = Uuid::new_v4();
        let embedding_json = entry.embedding.map(|e| serde_json::to_string(&e)).transpose()?;
        
        let row = sqlx::query_as::<_, MemoryEntry>(
            r#"
            INSERT INTO memory_entries (id, project_id, tier, content, embedding, metadata, confidence_score, source_entry_ids)
            VALUES ($1, $2, $3, $4, $5::jsonb, $6, $7, $8)
            RETURNING id, project_id, tier::text, content, embedding::text, metadata, confidence_score, decay_score, archived, source_entry_ids, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(entry.project_id)
        .bind(entry.tier.to_string())
        .bind(&entry.content)
        .bind(&embedding_json)
        .bind(&entry.metadata)
        .bind(entry.confidence_score.unwrap_or(0.5))
        .bind(&entry.source_entry_ids)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    pub async fn get(&self, id: Uuid) -> anyhow::Result<Option<MemoryEntry>> {
        let row = sqlx::query_as::<_, MemoryEntry>(
            r#"
            SELECT id, project_id, tier::text, content, embedding::text, metadata, confidence_score, decay_score, archived, source_entry_ids, created_at, updated_at
            FROM memory_entries
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    pub async fn update(&self, id: Uuid, update: UpdateMemoryEntry) -> anyhow::Result<Option<MemoryEntry>> {
        if let Some(content) = update.content {
            sqlx::query("UPDATE memory_entries SET content = $1, updated_at = NOW() WHERE id = $2")
                .bind(content)
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        if let Some(metadata) = update.metadata {
            sqlx::query("UPDATE memory_entries SET metadata = $1, updated_at = NOW() WHERE id = $2")
                .bind(metadata)
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        if let Some(confidence_score) = update.confidence_score {
            sqlx::query("UPDATE memory_entries SET confidence_score = $1, updated_at = NOW() WHERE id = $2")
                .bind(confidence_score)
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        if let Some(archived) = update.archived {
            sqlx::query("UPDATE memory_entries SET archived = $1, updated_at = NOW() WHERE id = $2")
                .bind(archived)
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        
        self.get(id).await
    }
    
    pub async fn delete(&self, id: Uuid) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM memory_entries WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
    
    pub async fn list(&self, filter: MemoryFilter, limit: i64, offset: i64) -> anyhow::Result<Vec<MemoryEntry>> {
        let mut query = String::from(
            r#"
            SELECT id, project_id, tier::text, content, embedding::text, metadata, confidence_score, decay_score, archived, source_entry_ids, created_at, updated_at
            FROM memory_entries
            WHERE 1=1
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
        if let Some(archived) = filter.archived {
            query.push_str(&format!(" AND archived = {}", archived));
        }
        if let Some(q) = filter.query {
            query.push_str(&format!(" AND content ILIKE '%{}%'", q.replace('\'', "''")));
        }
        
        query.push_str(" ORDER BY created_at DESC");
        query.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));
        
        let rows = sqlx::query_as::<_, MemoryEntry>(&query)
            .fetch_all(&self.pool)
            .await?;
        
        Ok(rows)
    }
    
    pub async fn search_by_embedding(
        &self,
        query_embedding: &[f32],
        filter: MemoryFilter,
        top_k: usize,
    ) -> anyhow::Result<Vec<(MemoryEntry, f32)>> {
        let embedding_json = serde_json::to_string(query_embedding)?;
        
        let mut query = String::from(
            r#"
            SELECT id, project_id, tier::text, content, embedding::text, metadata, confidence_score, decay_score, archived, source_entry_ids, created_at, updated_at,
                   1 - (embedding <=> $1::vector) as score
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
        
        let rows = sqlx::query_as::<_, SearchMemoryRow>(&query)
            .bind(&embedding_json)
            .fetch_all(&self.pool)
            .await?;
        
        let results: Vec<(MemoryEntry, f32)> = rows
            .into_iter()
            .map(|row| {
                let entry = MemoryEntry {
                    id: row.id,
                    project_id: row.project_id,
                    tier: row.tier,
                    content: row.content,
                    embedding: row.embedding,
                    metadata: row.metadata,
                    confidence_score: row.confidence_score,
                    decay_score: row.decay_score,
                    archived: row.archived,
                    source_entry_ids: row.source_entry_ids,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                };
                (entry, row.score)
            })
            .collect();
        
        Ok(results)
    }
    
    pub async fn consolidate(&self, source_ids: &[Uuid], consolidated_content: &str, embedding: &[f32], metadata: serde_json::Value) -> anyhow::Result<MemoryEntry> {
        let id = Uuid::new_v4();
        let embedding_json = serde_json::to_string(embedding)?;
        
        let row = sqlx::query_as::<_, MemoryEntry>(
            r#"
            INSERT INTO memory_entries (id, project_id, tier, content, embedding, metadata, confidence_score, source_entry_ids)
            SELECT $1, project_id, 'consolidated', $2, $3::vector, $4, 0.7, $5
            FROM memory_entries
            WHERE id = $6
            RETURNING id, project_id, tier::text, content, embedding::text, metadata, confidence_score, decay_score, archived, source_entry_ids, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(consolidated_content)
        .bind(&embedding_json)
        .bind(&metadata)
        .bind(source_ids)
        .bind(source_ids.first().unwrap_or(&Uuid::nil()))
        .fetch_one(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    pub async fn promote_to_verified(&self, id: Uuid, _evidence_id: Uuid) -> anyhow::Result<()> {
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
            SET decay_score = decay_score * 0.95, updated_at = NOW()
            WHERE tier = 'raw'
            AND archived = false
            AND decay_score > 0.1
            "#,
        )
        .execute(&self.pool)
        .await?;
        
        Ok(result.rows_affected() as usize)
    }
    
    pub async fn archive_low_decay(&self, threshold: f32) -> anyhow::Result<usize> {
        let result = sqlx::query(
            r#"
            UPDATE memory_entries
            SET archived = true, updated_at = NOW()
            WHERE tier = 'raw'
            AND archived = false
            AND decay_score < $1
            "#,
        )
        .bind(threshold)
        .execute(&self.pool)
        .await?;
        
        Ok(result.rows_affected() as usize)
    }
}
