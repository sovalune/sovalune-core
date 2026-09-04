use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;

#[derive(Deserialize)]
pub struct ListParams {
    pub project_id: Option<Uuid>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct StartCycleRequest {
    pub project_id: Uuid,
    pub origin_task_id: Uuid,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    let limit = params.limit.unwrap_or(20).min(100);
    let offset = params.offset.unwrap_or(0);

    match state
        .learning
        .list_cycles(
            params.project_id,
            params.status.and_then(|s| s.parse().ok()),
            limit,
            offset,
        )
        .await
    {
        Ok(cycles) => {
            let response: Vec<Value> = cycles
                .into_iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "project_id": c.project_id,
                        "status": c.status.to_string(),
                        "origin_task_id": c.origin_task_id,
                        "failure_reason": c.failure_reason,
                        "retry_count": c.retry_count,
                        "confidence_score": c.confidence_score,
                        "created_at": c.created_at.to_rfc3339(),
                        "updated_at": c.updated_at.to_rfc3339(),
                    })
                })
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

pub async fn start(
    State(state): State<Arc<AppState>>,
    Json(body): Json<StartCycleRequest>,
) -> Result<(axum::http::StatusCode, Json<Value>), (axum::http::StatusCode, Json<Value>)> {
    match state
        .learning
        .start_cycle(body.project_id, body.origin_task_id)
        .await
    {
        Ok(cycle) => Ok((
            axum::http::StatusCode::CREATED,
            Json(json!({
                "id": cycle.id,
                "project_id": cycle.project_id,
                "status": cycle.status.to_string(),
                "origin_task_id": cycle.origin_task_id,
                "created_at": cycle.created_at.to_rfc3339(),
            })),
        )),
        Err(e) => {
            let error_msg = e.to_string();
            let status = if error_msg.contains("Circuit breaker") {
                axum::http::StatusCode::TOO_MANY_REQUESTS
            } else {
                axum::http::StatusCode::BAD_REQUEST
            };
            Err((
                status,
                Json(json!({ "error": { "code": "START_FAILED", "message": error_msg } })),
            ))
        }
    }
}

pub async fn advance(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    match state.learning.advance_cycle(id).await {
        Ok(cycle) => Ok(Json(json!({
            "id": cycle.id,
            "status": cycle.status.to_string(),
            "retry_count": cycle.retry_count,
            "updated_at": cycle.updated_at.to_rfc3339(),
        }))),
        Err(e) => {
            let error_msg = e.to_string();
            let status = if error_msg.contains("terminal") {
                axum::http::StatusCode::CONFLICT
            } else if error_msg.contains("Max retries") {
                axum::http::StatusCode::GONE
            } else {
                axum::http::StatusCode::BAD_REQUEST
            };
            Err((
                status,
                Json(json!({ "error": { "code": "ADVANCE_FAILED", "message": error_msg } })),
            ))
        }
    }
}

pub async fn retry(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    match state.learning.retry_cycle(id).await {
        Ok(cycle) => Ok(Json(json!({
            "id": cycle.id,
            "status": cycle.status.to_string(),
            "retry_count": cycle.retry_count,
            "updated_at": cycle.updated_at.to_rfc3339(),
        }))),
        Err(e) => {
            let error_msg = e.to_string();
            Err((
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "error": { "code": "RETRY_FAILED", "message": error_msg } })),
            ))
        }
    }
}

pub async fn run(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    // Run the full self-learning pipeline for a cycle
    let cycle = match state.learning.get_cycle(id).await {
        Ok(c) => c,
        Err(e) => {
            return Err((
                axum::http::StatusCode::NOT_FOUND,
                Json(json!({ "error": { "code": "NOT_FOUND", "message": e.to_string() } })),
            ));
        }
    };

    let stages = [
        "detected",
        "researching",
        "verifying",
        "practicing",
        "testing",
        "applying",
    ];
    let stage_handlers: Vec<Box<dyn sovalune_self_learning::stages::StageHandler + Send + Sync>> = vec![
        Box::new(sovalune_self_learning::stages::detected::DetectedHandler),
        Box::new(sovalune_self_learning::stages::researching::ResearchingHandler::new(5, 60)),
        Box::new(sovalune_self_learning::stages::verifying::VerifyingHandler::new(0.6)),
        Box::new(sovalune_self_learning::stages::practicing::PracticingHandler),
        Box::new(sovalune_self_learning::stages::testing::TestingHandler::new(0.7)),
        Box::new(sovalune_self_learning::stages::applying::ApplyingHandler),
    ];

    let mut last_stage = cycle.status.to_string();
    let mut iterations = 0;

    while iterations < stages.len() {
        iterations += 1;
        let current = match state.learning.get_cycle(id).await {
            Ok(c) => c,
            Err(_) => break,
        };

        if current.status.is_terminal() {
            break;
        }

        let stage_idx = stages
            .iter()
            .position(|s| *s == current.status.to_string())
            .unwrap_or(0);

        let handler = &stage_handlers[stage_idx];
        match handler.execute(&state.learning, id).await {
            Ok(()) => {
                last_stage = current.status.to_string();
            }
            Err(e) => {
                tracing::error!("Stage {} failed: {}", handler.stage_name(), e);
                let _ = state.learning.fail_cycle(id, &e.to_string()).await;
                break;
            }
        }
    }

    let final_cycle = state.learning.get_cycle(id).await;
    match final_cycle {
        Ok(c) => Ok(Json(json!({
            "id": c.id,
            "status": c.status.to_string(),
            "confidence_score": c.confidence_score,
            "retry_count": c.retry_count,
            "last_stage": last_stage,
            "iterations": iterations,
        }))),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "code": "RUN_FAILED", "message": e.to_string() } })),
        )),
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
                .map(|e| {
                    json!({
                        "id": e.id,
                        "cycle_id": e.cycle_id,
                        "source_type": e.source_type,
                        "source_url": e.source_url,
                        "excerpt": e.excerpt,
                        "trust_tier": e.trust_tier,
                        "created_at": e.created_at.to_rfc3339(),
                    })
                })
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
                .map(|r| {
                    json!({
                        "id": r.id,
                        "cycle_id": r.cycle_id,
                        "stage": r.stage,
                        "passed": r.passed,
                        "detail": r.detail,
                        "created_at": r.created_at.to_rfc3339(),
                    })
                })
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
