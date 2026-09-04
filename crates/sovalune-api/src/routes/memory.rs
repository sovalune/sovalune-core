use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;
use sovalune_storage_client::{MemoryFilter, MemoryRepository, UpdateMemoryEntry};

#[derive(Deserialize)]
pub struct ListParams {
    pub project_id: Option<Uuid>,
    pub tier: Option<String>,
    pub q: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    let repo = MemoryRepository::new(state.storage.pool().clone());
    let limit = params.limit.unwrap_or(20).min(100);
    let offset = params.offset.unwrap_or(0);

    let tier = params.tier.and_then(|t| t.parse().ok());

    let filter = MemoryFilter {
        project_id: params.project_id,
        tier,
        min_confidence: None,
        archived: Some(false),
        query: params.q,
    };

    match repo.list(filter, limit, offset).await {
        Ok(entries) => {
            let response: Vec<Value> = entries
                .into_iter()
                .map(|e| {
                    json!({
                        "id": e.id,
                        "project_id": e.project_id,
                        "tier": e.tier,
                        "content": e.content,
                        "metadata": e.metadata,
                        "confidence_score": e.confidence_score,
                        "decay_score": e.decay_score,
                        "archived": e.archived,
                        "created_at": e.created_at.to_rfc3339(),
                        "updated_at": e.updated_at.to_rfc3339(),
                    })
                })
                .collect();
            Ok(Json(json!({ "data": response })))
        }
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "code": "INTERNAL_ERROR", "message": e.to_string() } })),
        )),
    }
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    let repo = MemoryRepository::new(state.storage.pool().clone());

    match repo.get(id).await {
        Ok(Some(entry)) => Ok(Json(json!({
            "id": entry.id,
            "project_id": entry.project_id,
            "tier": entry.tier,
            "content": entry.content,
            "metadata": entry.metadata,
            "confidence_score": entry.confidence_score,
            "decay_score": entry.decay_score,
            "archived": entry.archived,
            "source_entry_ids": entry.source_entry_ids,
            "created_at": entry.created_at.to_rfc3339(),
            "updated_at": entry.updated_at.to_rfc3339(),
        }))),
        Ok(None) => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(json!({ "error": { "code": "NOT_FOUND", "message": "Memory entry not found" } })),
        )),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "code": "INTERNAL_ERROR", "message": e.to_string() } })),
        )),
    }
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    let repo = MemoryRepository::new(state.storage.pool().clone());

    let update = UpdateMemoryEntry {
        content: body
            .get("content")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        metadata: body.get("metadata").cloned(),
        confidence_score: body
            .get("confidence_score")
            .and_then(|v| v.as_f64())
            .map(|f| f as f32),
        archived: body.get("archived").and_then(|v| v.as_bool()),
    };

    match repo.update(id, update).await {
        Ok(Some(entry)) => Ok(Json(json!({
            "id": entry.id,
            "project_id": entry.project_id,
            "tier": entry.tier,
            "content": entry.content,
            "metadata": entry.metadata,
            "confidence_score": entry.confidence_score,
            "decay_score": entry.decay_score,
            "archived": entry.archived,
            "created_at": entry.created_at.to_rfc3339(),
            "updated_at": entry.updated_at.to_rfc3339(),
        }))),
        Ok(None) => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(json!({ "error": { "code": "NOT_FOUND", "message": "Memory entry not found" } })),
        )),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "code": "INTERNAL_ERROR", "message": e.to_string() } })),
        )),
    }
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    let repo = MemoryRepository::new(state.storage.pool().clone());

    match repo.delete(id).await {
        Ok(()) => Ok(Json(json!({ "status": "deleted" }))),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "code": "INTERNAL_ERROR", "message": e.to_string() } })),
        )),
    }
}
