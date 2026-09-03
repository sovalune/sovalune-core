use axum::extract::{Path, State, Query};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;
use std::sync::Arc;

use crate::AppState;

#[derive(Deserialize)]
pub struct ListParams {
    pub project_id: Option<Uuid>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    let limit = params.limit.unwrap_or(20).min(100);
    let offset = params.offset.unwrap_or(0);
    
    match state.learning.list_cycles(
        params.project_id,
        params.status.and_then(|s| s.parse().ok()),
        limit,
        offset,
    ).await {
        Ok(cycles) => {
            let response: Vec<Value> = cycles
                .into_iter()
                .map(|c| json!({
                    "id": c.id,
                    "project_id": c.project_id,
                    "status": c.status.to_string(),
                    "origin_task_id": c.origin_task_id,
                    "failure_reason": c.failure_reason,
                    "retry_count": c.retry_count,
                    "confidence_score": c.confidence_score,
                    "created_at": c.created_at.to_rfc3339(),
                    "updated_at": c.updated_at.to_rfc3339(),
                }))
                .collect();
            Ok(Json(json!({ "data": response })))
        }
        Err(e) => {
            let error_msg = e.to_string();
            Err((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": { "code": "INTERNAL_ERROR", "message": error_msg } })),
            ))
        }
    }
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    match state.learning.get_cycle(id).await {
        Ok(cycle) => Ok(Json(json!({
            "id": cycle.id,
            "project_id": cycle.project_id,
            "status": cycle.status.to_string(),
            "origin_task_id": cycle.origin_task_id,
            "failure_reason": cycle.failure_reason,
            "retry_count": cycle.retry_count,
            "confidence_score": cycle.confidence_score,
            "created_at": cycle.created_at.to_rfc3339(),
            "updated_at": cycle.updated_at.to_rfc3339(),
        }))),
        Err(e) => {
            let error_msg = e.to_string();
            Err((
                axum::http::StatusCode::NOT_FOUND,
                Json(json!({ "error": { "code": "NOT_FOUND", "message": error_msg } })),
            ))
        }
    }
}

pub async fn evidence(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    match state.learning.get_evidence(id).await {
        Ok(evidence) => {
            let response: Vec<Value> = evidence
                .into_iter()
                .map(|e| json!({
                    "id": e.id,
                    "cycle_id": e.cycle_id,
                    "source_type": e.source_type,
                    "source_url": e.source_url,
                    "excerpt": e.excerpt,
                    "trust_tier": e.trust_tier,
                    "created_at": e.created_at.to_rfc3339(),
                }))
                .collect();
            Ok(Json(json!({ "data": response })))
        }
        Err(e) => {
            let error_msg = e.to_string();
            Err((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": { "code": "INTERNAL_ERROR", "message": error_msg } })),
            ))
        }
    }
}

pub async fn test_results(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    match state.learning.get_test_results(id).await {
        Ok(results) => {
            let response: Vec<Value> = results
                .into_iter()
                .map(|r| json!({
                    "id": r.id,
                    "cycle_id": r.cycle_id,
                    "stage": r.stage,
                    "passed": r.passed,
                    "detail": r.detail,
                    "created_at": r.created_at.to_rfc3339(),
                }))
                .collect();
            Ok(Json(json!({ "data": response })))
        }
        Err(e) => {
            let error_msg = e.to_string();
            Err((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": { "code": "INTERNAL_ERROR", "message": error_msg } })),
            ))
        }
    }
}
