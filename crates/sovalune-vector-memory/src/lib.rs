use serde::{Deserialize, Serialize};
use sovalune_storage_client::MemoryEntry;
use std::sync::Arc;

pub mod context_weaver;
pub mod store;

pub use context_weaver::ContextWeaver;
pub use store::VectorMemoryStore;

use sovalune_model_runtime::EmbeddingBackend;

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

/// Расширенное хранилище векторной памяти с поддержкой эмбеддингов.
#[derive(Clone)]
pub struct EmbeddingVectorMemoryStore {
    store: VectorMemoryStore,
    embedding_backend: std::sync::Arc<dyn EmbeddingBackend>,
}

impl EmbeddingVectorMemoryStore {
    /// Создаёт новое хранилище с эмбеддинг-бэкендом.
    pub fn new(store: VectorMemoryStore, embedding_backend: Arc<dyn EmbeddingBackend>) -> Self {
        Self {
            store,
            embedding_backend,
        }
    }

    /// Вставляет запись с автоматической генерацией эмбеддинга.
    pub async fn insert_raw_with_embedding(
        &self,
        project_id: uuid::Uuid,
        content: &str,
        metadata: serde_json::Value,
    ) -> anyhow::Result<uuid::Uuid> {
        let embedding = self.embedding_backend.embed(content).await?;
        self.store
            .insert_raw(project_id, content, &embedding, metadata)
            .await
    }

    /// Ищет по тексту с автоматической генерацией эмбеддинга запроса.
    pub async fn search_by_text_with_embedding(
        &self,
        query: &str,
        filter: sovalune_storage_client::MemoryFilter,
        top_k: usize,
    ) -> anyhow::Result<Vec<ScoredMemory>> {
        let query_embedding = self.embedding_backend.embed(query).await?;
        self.store.search(&query_embedding, filter, top_k).await
    }

    /// Консолидирует записи с генерацией эмбеддинга.
    pub async fn consolidate_with_embedding(
        &self,
        source_ids: &[uuid::Uuid],
    ) -> anyhow::Result<uuid::Uuid> {
        // Загружаем исходные записи
        let mut contents = Vec::new();
        for id in source_ids {
            if let Some(entry) = self.store.get_entry(*id).await? {
                contents.push(entry.content.clone());
            }
        }

        if contents.is_empty() {
            return Err(anyhow::anyhow!("No valid source entries found"));
        }

        // Объединяем содержимое
        let consolidated = format!(
            "Consolidated from {} sources:\n{}",
            contents.len(),
            contents.join("\n---\n")
        );

        // Генерируем эмбеддинг
        let embedding = self.embedding_backend.embed(&consolidated).await?;

        self.store.consolidate(source_ids, &embedding).await
    }

    /// Возвращает ссылку на внутреннее хранилище.
    pub fn inner(&self) -> &VectorMemoryStore {
        &self.store
    }

    /// Возвращает бэкенд эмбеддингов.
    pub fn embedding_backend(&self) -> &dyn EmbeddingBackend {
        self.embedding_backend.as_ref()
    }
}

/// Фабрика хранилищ векторной памяти.
pub struct VectorMemoryFactory;

impl VectorMemoryFactory {
    /// Создаёт хранилище с указанным эмбеддинг-бэкендом.
    pub async fn create(
        pool: sqlx::PgPool,
        embedding_backend: Arc<dyn EmbeddingBackend>,
    ) -> anyhow::Result<EmbeddingVectorMemoryStore> {
        let store = VectorMemoryStore::new(pool);
        Ok(EmbeddingVectorMemoryStore::new(store, embedding_backend))
    }

    /// Создаёт хранилище из конфигурации.
    pub async fn from_config(
        pool: sqlx::PgPool,
        backend_type: &str,
        api_url: &str,
        api_key: Option<&str>,
        model: &str,
        dimensions: usize,
    ) -> anyhow::Result<EmbeddingVectorMemoryStore> {
        let embedding_backend = sovalune_model_runtime::EmbeddingFactory::create(
            backend_type,
            api_url,
            api_key,
            model,
            dimensions,
        )
        .await?;

        let store = VectorMemoryStore::new(pool);
        Ok(EmbeddingVectorMemoryStore::new(
            store,
            Arc::from(embedding_backend),
        ))
    }
}
