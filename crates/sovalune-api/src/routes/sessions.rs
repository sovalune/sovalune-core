use axum::extract::{Path, State, Query};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;
use std::sync::Arc;

use crate::AppState;
use sovalune_storage_client::{SessionRepository, CreateMessage};

#[derive(Deserialize)]
pub struct ListParams {
    pub project_id: Option<Uuid>,
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
    
    match params.project_id {
        Some(project_id) => {
            match repo.list_sessions(project_id, limit, offset).await {
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
        None => {
            Err((
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "error": { "code": "VALIDATION_ERROR", "message": "project_id is required" } })),
            ))
        }
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

pub async fn add_message(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    let role = body.get("role")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "error": { "code": "VALIDATION_ERROR", "message": "role is required" } })),
            )
        })?;
    
    let content = body.get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "error": { "code": "VALIDATION_ERROR", "message": "content is required" } })),
            )
        })?;
    
    let request_id = body.get("request_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::new_v4);
    
    let tool_call = body.get("tool_call").cloned();
    
    let repo = SessionRepository::new(state.storage.pool().clone());
    
    match repo.create_message(CreateMessage {
        session_id: id,
        role: role.to_string(),
        content: content.to_string(),
        tool_call,
        request_id,
    }).await {
        Ok(message) => Ok(Json(json!({
            "id": message.id,
            "session_id": message.session_id,
            "role": message.role,
            "content": message.content,
            "tool_call": message.tool_call,
            "request_id": message.request_id,
            "created_at": message.created_at.to_rfc3339(),
        }))),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "code": "INTERNAL_ERROR", "message": e.to_string() } })),
        )),
    }
}
