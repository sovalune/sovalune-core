use axum::extract::{Path, State, Query};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;
use std::sync::Arc;

use crate::AppState;
use sovalune_storage_client::SessionRepository;

#[derive(Deserialize)]
pub struct ListParams {
    pub project_id: Uuid,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    let repo = SessionRepository::new(state.storage.pool().clone());
    let limit = params.limit.unwrap_or(20).min(100);
    let offset = params.offset.unwrap_or(0);
    
    match repo.list_sessions(params.project_id, limit, offset).await {
        Ok(sessions) => {
            let response: Vec<Value> = sessions
                .into_iter()
                .map(|s| json!({
                    "id": s.id,
                    "project_id": s.project_id,
                    "created_at": s.created_at.to_rfc3339(),
                }))
                .collect();
            Ok(Json(json!({ "data": response })))
        }
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "code": "INTERNAL_ERROR", "message": e.to_string() } })),
        )),
    }
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    let project_id = body.get("project_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "error": { "code": "VALIDATION_ERROR", "message": "project_id is required" } })),
            )
        })?;
    
    let repo = SessionRepository::new(state.storage.pool().clone());
    
    match repo.create_session(project_id).await {
        Ok(session) => Ok(Json(json!({
            "id": session.id,
            "project_id": session.project_id,
            "created_at": session.created_at.to_rfc3339(),
        }))),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "code": "INTERNAL_ERROR", "message": e.to_string() } })),
        )),
    }
}

pub async fn messages(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    let repo = SessionRepository::new(state.storage.pool().clone());
    
    match repo.get_messages(id, 100).await {
        Ok(messages) => {
            let response: Vec<Value> = messages
                .into_iter()
                .map(|m| json!({
                    "id": m.id,
                    "session_id": m.session_id,
                    "role": m.role,
                    "content": m.content,
                    "tool_call": m.tool_call,
                    "request_id": m.request_id,
                    "created_at": m.created_at.to_rfc3339(),
                }))
                .collect();
            Ok(Json(json!({ "data": response })))
        }
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "code": "INTERNAL_ERROR", "message": e.to_string() } })),
        )),
    }
}
