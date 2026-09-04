use async_trait::async_trait;
use sovalune_api::{create_router, AppState};
use sovalune_bus::NatsClient;
use sovalune_config::AppConfig;
use sovalune_model_runtime::{
    executors::{
        create_registry, MemorySearchBackend, MemorySearchResult, MemoryWriteBackend,
    },
    BackendConfig, CacheFactory, EmbeddingBackend, EmbeddingFactory, InferenceEngine, TokenCounter,
};
use sovalune_self_learning::LearningCycleOrchestrator;
use sovalune_storage_client::{MemoryFilter, StorageClient};
use sovalune_vector_memory::{EmbeddingVectorMemoryStore, VectorMemoryStore};
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sovalune_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Sovalune Server...");

    let config = AppConfig::from_env()?;
    info!("Configuration loaded");

    let storage = StorageClient::new(&config.storage_url).await?;
    info!("Connected to storage");

    storage.run_migrations().await?;
    info!("Migrations completed");

    let nats = NatsClient::new(&config.nats_url).await?;
    info!("Connected to NATS");

    // Initialize embedding backend
    let embedding_backend: Arc<dyn EmbeddingBackend> = match EmbeddingFactory::create(
        &config.model_backend,
        &config.model_api_url,
        config.model_api_key.as_deref(),
        &config.model_name,
        1536,
    )
    .await
    {
        Ok(backend) => {
            info!(
                "Embedding backend initialized: model={}, dimensions={}",
                backend.model_name(),
                backend.dimensions()
            );
            Arc::from(backend)
        }
        Err(e) => {
            warn!(
                "Failed to initialize embedding backend: {}. Using text search only.",
                e
            );
            Arc::new(StubEmbeddingBackend)
        }
    };

    // Initialize vector memory store with embeddings
    let vector_memory = EmbeddingVectorMemoryStore::new(
        VectorMemoryStore::new(storage.pool().clone()),
        embedding_backend.clone(),
    );
    info!("Vector memory store initialized with embedding support");

    // Initialize inference engine
    let inference_config = BackendConfig {
        backend_type: config.model_backend.clone(),
        api_url: config.model_api_url.clone(),
        api_key: config.model_api_key.clone(),
        model_name: config.model_name.clone(),
        timeout_secs: config.model_timeout_secs,
        max_retries: 3,
    };

    let inference_engine = match InferenceEngine::from_config(&inference_config).await {
        Ok(engine) => {
            info!(
                "Inference engine initialized: model={}, backend={}",
                engine.backend_info().model,
                config.model_backend
            );
            Arc::new(engine)
        }
        Err(e) => {
            warn!(
                "Failed to initialize inference engine: {}. Running in degraded mode.",
                e
            );
            Arc::new(InferenceEngine::new(Arc::new(StubBackend)))
        }
    };

    // Initialize tool registry with real memory backends
    let memory_search = Arc::new(RealMemorySearchBackend {
        vector_memory: vector_memory.clone(),
    });
    let memory_write = Arc::new(RealMemoryWriteBackend {
        vector_memory: vector_memory.clone(),
    });
    let tool_registry = Arc::new(create_registry(memory_search, memory_write));
    info!("Tool registry initialized: {} tools", tool_registry.len());

    let _token_counter = Arc::new(TokenCounter::new());
    info!("Token counter initialized");

    let _response_cache = Arc::new(CacheFactory::response_cache());
    let _embedding_cache = Arc::new(CacheFactory::embedding_cache());
    info!("Caches initialized");

    // Initialize learning cycle orchestrator with backends
    let learning = LearningCycleOrchestrator::with_backends(
        storage.pool().clone(),
        Some(nats.clone()),
        inference_engine.clone(),
        vector_memory.clone(),
    );
    info!("Learning cycle orchestrator initialized with backends");

    let state = AppState {
        storage: storage.clone(),
        nats: nats.clone(),
        vector_memory: vector_memory.clone(),
        learning: learning.clone(),
        inference: inference_engine,
        tool_registry,
    };

    // Background decay tick task
    let state_decay = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            info!("Running decay tick...");
            match state_decay.vector_memory.inner().decay_tick().await {
                Ok(affected) => info!("Decay tick: {} entries affected", affected),
                Err(e) => warn!("Decay tick failed: {}", e),
            }
            match state_decay
                .vector_memory
                .inner()
                .archive_low_decay(0.1)
                .await
            {
                Ok(archived) => {
                    if archived > 0 {
                        info!("Archived {} entries with low decay", archived);
                    }
                }
                Err(e) => warn!("Archive low decay failed: {}", e),
            }
        }
    });
    info!("Decay tick background task started");

    let app = create_router(state);

    let addr = format!("{}:{}", config.server_host, config.server_port);
    info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Stub backend for when real model backend is unavailable.
struct StubBackend;

#[async_trait]
impl sovalune_model_runtime::ModelBackend for StubBackend {
    async fn stream_inference(
        &self,
        _request: &sovalune_model_runtime::InferenceRequest,
    ) -> Result<
        futures::stream::BoxStream<
            '_,
            Result<sovalune_model_runtime::TokenEvent, sovalune_model_runtime::InferenceError>,
        >,
        sovalune_model_runtime::InferenceError,
    > {
        Err(sovalune_model_runtime::InferenceError::BackendUnavailable(
            "No model backend configured. Set SOVALUNE_MODEL_BACKEND environment variable.".into(),
        ))
    }

    fn model_name(&self) -> &str {
        "stub"
    }
    fn max_context_tokens(&self) -> usize {
        0
    }

    async fn health_check(&self) -> bool {
        false
    }

    async fn from_config(
        _config: &sovalune_model_runtime::BackendConfig,
    ) -> Result<Self, sovalune_model_runtime::InferenceError>
    where
        Self: Sized,
    {
        Ok(Self)
    }
}

/// Stub embedding backend.
struct StubEmbeddingBackend;

#[async_trait]
impl EmbeddingBackend for StubEmbeddingBackend {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, sovalune_model_runtime::EmbeddingError> {
        let mut embedding = vec![0.0f32; 128];
        for (i, byte) in text.bytes().enumerate() {
            embedding[i % 128] += byte as f32;
        }
        let sum: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if sum > 0.0 {
            for x in &mut embedding {
                *x /= sum;
            }
        }
        Ok(embedding)
    }

    async fn embed_batch(
        &self,
        texts: &[&str],
    ) -> Result<Vec<Vec<f32>>, sovalune_model_runtime::EmbeddingError> {
        let mut results = Vec::new();
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    fn dimensions(&self) -> usize {
        128
    }
    fn model_name(&self) -> &str {
        "stub-hash"
    }

    async fn health_check(&self) -> bool {
        true
    }
}

/// Real memory search backend wrapping the vector memory store.
struct RealMemorySearchBackend {
    vector_memory: EmbeddingVectorMemoryStore,
}

#[async_trait]
impl MemorySearchBackend for RealMemorySearchBackend {
    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<MemorySearchResult>, String> {
        let filter = MemoryFilter {
            project_id: None,
            tier: None,
            min_confidence: Some(0.3),
            archived: Some(false),
            query: Some(query.to_string()),
        };

        match self
            .vector_memory
            .search_by_text_with_embedding(query, filter, top_k)
            .await
        {
            Ok(scored) => Ok(scored
                .into_iter()
                .map(|sm| MemorySearchResult {
                    content: sm.entry.content,
                    score: sm.score,
                    tier: sm.entry.tier,
                })
                .collect()),
            Err(e) => Err(format!("Search failed: {}", e)),
        }
    }
}

/// Real memory write backend wrapping the vector memory store.
struct RealMemoryWriteBackend {
    vector_memory: EmbeddingVectorMemoryStore,
}

#[async_trait]
impl MemoryWriteBackend for RealMemoryWriteBackend {
    async fn write(&self, content: &str, metadata: serde_json::Value) -> Result<String, String> {
        let project_id = Uuid::nil();

        match self
            .vector_memory
            .insert_raw_with_embedding(project_id, content, metadata)
            .await
        {
            Ok(id) => Ok(id.to_string()),
            Err(e) => Err(format!("Write failed: {}", e)),
        }
    }
}
