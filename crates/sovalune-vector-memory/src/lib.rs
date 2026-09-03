use sqlx::PgPool;
use uuid::Uuid;

mod store;
mod context_weaver;

pub use store::VectorMemoryStore;
pub use context_weaver::ContextWeaver;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RawMemoryEntry {
    pub content: String,
    pub embedding: Vec<f32>,
    pub metadata: serde_json::Value,
    pub project_id: Uuid,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryEntry {
    pub id: Uuid,
    pub project_id: Uuid,
    pub tier: MemoryTier,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub metadata: serde_json::Value,
    pub confidence_score: f32,
    pub decay_score: f32,
    pub archived: bool,
    pub source_entry_ids: Option<Vec<Uuid>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScoredMemory {
    pub entry: MemoryEntry,
    pub score: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryFilter {
    pub project_id: Option<Uuid>,
    pub tier: Option<MemoryTier>,
    pub min_confidence: Option<f32>,
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Evidence {
    pub source_type: String,
    pub source_url: Option<String>,
    pub excerpt: String,
    pub trust_tier: i32,
}
