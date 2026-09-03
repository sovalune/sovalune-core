use sovalune_storage_client::MemoryEntry;
use serde::{Deserialize, Serialize};

pub mod store;
pub mod context_weaver;

pub use store::VectorMemoryStore;
pub use context_weaver::ContextWeaver;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredMemory {
    pub entry: MemoryEntry,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub source_type: String,
    pub source_url: Option<String>,
    pub excerpt: String,
    pub trust_tier: i32,
}
