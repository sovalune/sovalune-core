//! SSE и статус инференса.
//!
//! `POST /api/v1/sessions/:id/infer` — streaming inference через SSE.
//! `GET  /api/v1/inference/status` — статус бэкенда моделей.

use axum::{
    extract::{Path, State},
    response::{
        sse::{Event, KeepAlive, Sse},
    },
    Json,
};
use futures::{Stream, StreamExt};
use serde::Serialize;
use std::convert::Infallible;
use uuid::Uuid;

use crate::AppState;
use sovalune_model_runtime::{ContextBuilder, GenerationConfig, InferenceRequest};
use sovalune_storage_client::{SessionRepository, MemoryFilter, CreateMessage};

/// Статус инференса.
#[derive(Serialize)]
pub struct InferenceStatus {
    pub backend: String,
    pub model: String,
    pub healthy: bool,
    pub max_context_tokens: usize,
}

/// `GET /api/v1/inference/status`
pub async fn status(State(state): State<std::sync::Arc<AppState>>) -> Json<InferenceStatus> {
    let healthy = state.inference.health_check().await;
    let info = state.inference.backend_info();

    Json(InferenceStatus {
        backend: std::env::var("SOVALUNE_MODEL_BACKEND").unwrap_or_else(|_| "none".to_string()),
        model: info.model,
        healthy,
        max_context_tokens: info.max_context_tokens,
    })
}

/// Тело запроса на инференс.
#[derive(serde::Deserialize)]
pub struct InferRequest {
    pub message: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

fn default_max_tokens() -> u32 { 2048 }
fn default_temperature() -> f32 { 0.7 }

/// `POST /api/v1/sessions/:id/infer`
///
/// Streaming inference через Server-Sent Events:
/// ```text
/// data: {"type":"token","content":"Hello"}
/// data: {"type":"token","content":" world"}
/// data: {"type":"done","usage":{"prompt_tokens":100,"completion_tokens":50}}
/// ```
pub async fn infer(
    State(state): State<std::sync::Arc<AppState>>,
    Path(session_id): Path<Uuid>,
    Json(req): Json<InferRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let inference_request_id = Uuid::new_v4();

    // Сохраняем сообщение пользователя
    let session_repo = SessionRepository::new(state.storage.pool().clone());
    let _ = session_repo
        .create_message(CreateMessage {
            session_id,
            role: "user".to_string(),
            content: req.message.clone(),
            tool_call: None,
            request_id: inference_request_id,
        })
        .await;

    // Ищем релевантную память через эмбеддинги
    let memory_filter = MemoryFilter {
        project_id: None,
        tier: None,
        min_confidence: Some(0.5),
        archived: Some(false),
        query: Some(req.message.clone()),
    };

    let memories = match state.vector_memory
        .search_by_text_with_embedding(&req.message, memory_filter, 10)
        .await
    {
        Ok(scored) => {
            scored.into_iter().map(|sm| sm.entry).collect()
        }
        Err(e) => {
            // Fallback to text search
            let memory_repo = sovalune_storage_client::MemoryRepository::new(state.storage.pool().clone());
            let fallback_filter = MemoryFilter {
                project_id: None,
                tier: None,
                min_confidence: Some(0.5),
                archived: Some(false),
                query: Some(req.message.clone()),
            };
            match memory_repo.list(fallback_filter, 10, 0).await {
                Ok(entries) => entries,
                Err(_) => Vec::new(),
            }
        }
    };

    // Загружаем историю сессии
    let history = session_repo
        .get_recent_messages(session_id, 50)
        .await
        .unwrap_or_default();

    // Собираем контекст
    let mut builder = ContextBuilder::new(128_000)
        .with_system("You are Sovalune, an AI assistant with long-term memory.");

    for entry in &memories {
        builder = builder.with_memory_section(&entry.tier, &entry.content);
    }

    for msg in &history {
        builder = builder.with_history_entry(&msg.role, &msg.content);
    }

    builder = builder.with_user_input(&req.message);

    let context = builder.build();

    let request = InferenceRequest {
        id: inference_request_id,
        session_id: session_id.to_string(),
        project_id: String::new(),
        context,
        user_input: req.message.clone(),
        config: GenerationConfig {
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            top_p: 0.9,
            stream: true,
        },
    };

    let inference = state.inference.clone();
    let storage = state.storage.clone();

    let stream = async_stream::stream! {
        match inference.stream_infer(request).await {
            Ok(mut token_stream) => {
                let mut full_response = String::new();

                while let Some(event_result) = token_stream.next().await {
                    match event_result {
                        Ok(token) => {
                            if token.finished {
                                // Сохраняем ответ ассистента
                                let session_repo = SessionRepository::new(storage.pool().clone());
                                let _ = session_repo.create_message(CreateMessage {
                                    session_id,
                                    role: "assistant".to_string(),
                                    content: full_response.clone(),
                                    tool_call: None,
                                    request_id: Uuid::new_v4(),
                                }).await;

                                let done_event = serde_json::json!({
                                    "type": "done",
                                    "content": "",
                                    "finished": true
                                });
                                yield Ok(Event::default().data(done_event.to_string()));
                            } else {
                                full_response.push_str(&token.delta);
                                let token_event = serde_json::json!({
                                    "type": "token",
                                    "content": token.delta,
                                    "finished": false
                                });
                                yield Ok(Event::default().data(token_event.to_string()));
                            }
                        }
                        Err(e) => {
                            let error_event = serde_json::json!({
                                "type": "error",
                                "content": format!("{}", e),
                                "finished": true
                            });
                            yield Ok(Event::default().data(error_event.to_string()));
                            return;
                        }
                    }
                }
            }
            Err(e) => {
                let error_event = serde_json::json!({
                    "type": "error",
                    "content": format!("Inference failed: {}", e),
                    "finished": true
                });
                yield Ok(Event::default().data(error_event.to_string()));
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}
