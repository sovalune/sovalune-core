use axum::Json;
use serde_json::{json, Value};

pub async fn live() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

pub async fn ready(
    state: axum::extract::State<std::sync::Arc<crate::AppState>>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    let storage_ok = state.storage.health_check().await;
    let nats_ok = state.nats.health_check().await;

    if storage_ok && nats_ok {
        Ok(Json(json!({ "status": "ok" })))
    } else {
        Err((
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "error",
                "storage": storage_ok,
                "nats": nats_ok,
            })),
        ))
    }
}
