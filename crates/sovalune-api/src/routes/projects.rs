use axum::extract::{Path, State, Query};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;
use std::sync::Arc;

use crate::AppState;
use sovalune_storage_client::{ProjectRepository, CreateProject};

#[derive(Deserialize)]
pub struct ListParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Serialize)]
pub struct ProjectResponse {
    pub id: Uuid,
    pub name: String,
    pub settings: Value,
    pub created_at: String,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    let repo = ProjectRepository::new(state.storage.pool().clone());
    let limit = params.limit.unwrap_or(20).min(100);
    let offset = params.offset.unwrap_or(0);
    
    match repo.list(limit, offset).await {
        Ok(projects) => {
            let response: Vec<ProjectResponse> = projects
                .into_iter()
                .map(|p| ProjectResponse {
                    id: p.id,
                    name: p.name,
                    settings: p.settings,
                    created_at: p.created_at.to_rfc3339(),
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

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    let name = body.get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "error": { "code": "VALIDATION_ERROR", "message": "name is required" } })),
            )
        })?;
    
    let repo = ProjectRepository::new(state.storage.pool().clone());
    
    match repo.create(CreateProject {
        name: name.to_string(),
        settings: body.get("settings").cloned(),
    }).await {
        Ok(project) => Ok(Json(json!({
            "id": project.id,
            "name": project.name,
            "settings": project.settings,
            "created_at": project.created_at.to_rfc3339(),
        }))),
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
    let repo = ProjectRepository::new(state.storage.pool().clone());
    
    match repo.get(id).await {
        Ok(Some(project)) => Ok(Json(json!({
            "id": project.id,
            "name": project.name,
            "settings": project.settings,
            "created_at": project.created_at.to_rfc3339(),
        }))),
        Ok(None) => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(json!({ "error": { "code": "NOT_FOUND", "message": "Project not found" } })),
        )),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "code": "INTERNAL_ERROR", "message": e.to_string() } })),
        )),
    }
}
