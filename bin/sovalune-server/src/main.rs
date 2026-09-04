use sovalune_api::{create_router, AppState};
use sovalune_bus::NatsClient;
use sovalune_config::AppConfig;
use sovalune_storage_client::StorageClient;
use sovalune_vector_memory::VectorMemoryStore;
use sovalune_self_learning::LearningCycleOrchestrator;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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

    let vector_memory = VectorMemoryStore::new(storage.pool().clone());
    info!("Vector memory store initialized");

    let learning = LearningCycleOrchestrator::new(storage.pool().clone());
    info!("Learning cycle orchestrator initialized");

    let state = AppState {
        storage: storage.clone(),
        nats: nats.clone(),
        vector_memory: vector_memory.clone(),
        learning: learning.clone(),
    };

    let state_decay = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            info!("Running decay tick...");
            match state_decay.vector_memory.decay_tick().await {
                Ok(affected) => info!("Decay tick: {} entries affected", affected),
                Err(e) => warn!("Decay tick failed: {}", e),
            }
            match state_decay.vector_memory.archive_low_decay(0.1).await {
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
