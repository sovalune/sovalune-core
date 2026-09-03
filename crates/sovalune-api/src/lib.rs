pub mod routes;

use axum::{routing::get, Router};
use sovalune_bus::NatsClient;
use sovalune_storage_client::StorageClient;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub storage: StorageClient,
    pub nats: NatsClient,
}

pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health/live", get(routes::health::live))
        .route("/health/ready", get(routes::health::ready))
        .route("/api/v1/projects", get(routes::projects::list))
        .route("/api/v1/sessions", get(routes::sessions::list))
        .route("/api/v1/memory", get(routes::memory::list))
        .route("/api/v1/learning-cycles", get(routes::learning::list))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(Arc::new(state))
}
