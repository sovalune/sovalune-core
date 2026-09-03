use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

pub async fn list() -> Result<Json<Value>, StatusCode> {
    Ok(Json(json!({
        "learning_cycles": [],
        "next_cursor": null
    })))
}
