pub mod routes;

use axum::{routing::{get, post}, Router};
use sovalune_bus::NatsClient;
use sovalune_storage_client::StorageClient;
use sovalune_vector_memory::EmbeddingVectorMemoryStore;
use sovalune_self_learning::LearningCycleOrchestrator;
use sovalune_model_runtime::InferenceEngine;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub storage: StorageClient,
    pub nats: NatsClient,
    pub vector_memory: EmbeddingVectorMemoryStore,
    pub learning: LearningCycleOrchestrator,
    pub inference: Arc<InferenceEngine>,
}

pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .max_age(std::time::Duration::from_secs(3600));

    Router::new()
        .route("/health/live", get(routes::health::live))
        .route("/health/ready", get(routes::health::ready))
        .route("/api/v1/projects", get(routes::projects::list).post(routes::projects::create))
        .route("/api/v1/projects/{id}", get(routes::projects::get))
        .route("/api/v1/sessions", get(routes::sessions::list).post(routes::sessions::create))
        .route("/api/v1/sessions/{id}/messages", get(routes::sessions::messages).post(routes::sessions::add_message))
        .route("/api/v1/sessions/{id}/infer", post(routes::inference::infer))
        .route("/api/v1/memory", get(routes::memory::list))
        .route("/api/v1/memory/{id}", get(routes::memory::get).patch(routes::memory::update).delete(routes::memory::delete))
        .route("/api/v1/learning-cycles", get(routes::learning::list))
        .route("/api/v1/learning-cycles/{id}", get(routes::learning::get))
        .route("/api/v1/learning-cycles/{id}/evidence", get(routes::learning::evidence))
        .route("/api/v1/learning-cycles/{id}/test-results", get(routes::learning::test_results))
        .route("/api/v1/inference/status", get(routes::inference::status))
        .route("/ws/chat", get(routes::ws::ws_handler))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(Arc::new(state))
}
