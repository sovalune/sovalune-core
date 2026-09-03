use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::AppState;

pub async fn live() -> StatusCode {
    StatusCode::OK
}

pub async fn ready(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let storage_ok = state.storage.health_check().await;
    let nats_ok = state.nats.health_check().await;

    if storage_ok && nats_ok {
        Ok(StatusCode::OK)
    } else {
        let mut errors = Vec::new();
        if !storage_ok {
            errors.push("storage");
        }
        if !nats_ok {
            errors.push("nats");
        }
        
        Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": {
                    "code": "SERVICE_UNAVAILABLE",
                    "message": format!("Services not ready: {}", errors.join(", ")),
                }
            })),
        ))
    }
}
