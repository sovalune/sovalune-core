use sovalune_api::{create_router, AppState};
use sovalune_bus::NatsClient;
use sovalune_config::AppConfig;
use sovalune_storage_client::StorageClient;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sovalune_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Sovalune Server...");

    // Load configuration
    let config = AppConfig::from_env()?;
    info!("Configuration loaded");

    // Connect to storage
    let storage = StorageClient::new(&config.storage_url).await?;
    info!("Connected to storage");

    // Run migrations
    storage.run_migrations().await?;
    info!("Migrations completed");

    // Connect to NATS
    let nats = NatsClient::new(&config.nats_url).await?;
    info!("Connected to NATS");

    // Create app state
    let state = AppState {
        storage,
        nats,
    };

    // Create router
    let app = create_router(state);

    // Start server
    let addr = format!("{}:{}", config.server_host, config.server_port);
    info!("Server listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
