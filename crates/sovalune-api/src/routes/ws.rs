//! WebSocket обработчик для чата.
//!
//! Обрабатывает подключения клиентов через WebSocket:
//! - Сохраняет сообщения пользователя в БД
//! - Загружает контекст памяти и истории
//! - Собирает промпт через ContextBuilder
//! - Запускает инференс через InferenceEngine
//! - Стримит токены обратно клиенту
//! - Сохраняет ответ ассистента в БД

use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, error, warn};
use uuid::Uuid;

use crate::AppState;
use sovalune_model_runtime::{ContextBuilder, GenerationConfig, InferenceRequest, TokenEvent};
use sovalune_storage_client::{SessionRepository, MemoryRepository, CreateMessage, MemoryFilter};

/// Сообщение от клиента.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// Пользовательское сообщение.
    #[serde(rename = "user_message")]
    UserMessage {
        session_id: String,
        project_id: String,
        content: String,
    },
    /// Остановка генерации.
    #[serde(rename = "stop_generation")]
    StopGeneration { session_id: String },
    /// Присоединение к сессии.
    #[serde(rename = "join_session")]
    JoinSession {
        session_id: String,
        project_id: String,
    },
}

/// Сообщение сервера клиенту.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// Токен из стрима.
    #[serde(rename = "token")]
    Token { session_id: String, delta: String },
    /// Начало вызова инструмента.
    #[serde(rename = "tool_call_started")]
    ToolCallStarted {
        session_id: String,
        tool: String,
        arguments: serde_json::Value,
    },
    /// Завершение вызова инструмента.
    #[serde(rename = "tool_call_finished")]
    ToolCallFinished {
        session_id: String,
        tool: String,
        result_summary: String,
    },
    /// Сообщение сгенерировано.
    #[serde(rename = "message_complete")]
    MessageComplete {
        session_id: String,
        message_id: String,
    },
    /// Обновление learning cycle.
    #[serde(rename = "learning_cycle_update")]
    LearningCycleUpdate {
        cycle_id: String,
        status: String,
        detail: serde_json::Value,
    },
    /// Ошибка.
    #[serde(rename = "error")]
    Error { code: String, message: String },
}

/// Обработчик HTTP-запроса на WebSocket upgrade.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Основной обработчик WebSocket соединения.
///
/// Протокол:
/// 1. Клиент отправляет `join_session` для привязки к сессии
/// 2. Клиент отправляет `user_message` для генерации ответа
/// 3. Сервер стримит `token` события обратно
/// 4. При завершении — `message_complete`
async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (sender, mut receiver) = socket.split();
    let sender = Arc::new(Mutex::new(sender));
    let request_id = Uuid::new_v4().to_string();

    info!("WebSocket connected: {}", request_id);

    while let Some(msg) = receiver.next().await {
        let msg = match msg {
            Ok(msg) => msg,
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
        };

        match msg {
            Message::Text(text) => {
                let client_msg: Result<ClientMessage, _> = serde_json::from_str(&text);

                match client_msg {
                    Ok(ClientMessage::UserMessage {
                        session_id,
                        project_id,
                        content,
                    }) => {
                        info!(
                            "User message in session {}: {}",
                            session_id,
                            &content[..50.min(content.len())]
                        );

                        // Парсим UUID сессии и проекта
                        let session_uuid = match Uuid::parse_str(&session_id) {
                            Ok(id) => id,
                            Err(e) => {
                                send_error(&sender, "INVALID_SESSION", &format!("Invalid session ID: {}", e)).await;
                                continue;
                            }
                        };

                        let project_uuid = match Uuid::parse_str(&project_id) {
                            Ok(id) => id,
                            Err(e) => {
                                send_error(&sender, "INVALID_PROJECT", &format!("Invalid project ID: {}", e)).await;
                                continue;
                            }
                        };

                        let inference_request_id = Uuid::new_v4();
                        let session_repo = SessionRepository::new(state.storage.pool().clone());

                        // Сохраняем сообщение пользователя в БД
                        if let Err(e) = session_repo
                            .create_message(CreateMessage {
                                session_id: session_uuid,
                                role: "user".to_string(),
                                content: content.clone(),
                                tool_call: None,
                                request_id: inference_request_id,
                            })
                            .await
                        {
                            error!("Failed to save user message: {}", e);
                            send_error(&sender, "DB_ERROR", &format!("Failed to save message: {}", e)).await;
                            continue;
                        }

                        // Собираем контекст из памяти
                        let memory_repo = MemoryRepository::new(state.storage.pool().clone());
                        let memory_filter = MemoryFilter {
                            project_id: Some(project_uuid),
                            tier: None,
                            min_confidence: Some(0.5),
                            archived: Some(false),
                            query: Some(content.clone()),
                        };

                        let memory_entries = match memory_repo.list(memory_filter, 10, 0).await {
                            Ok(entries) => entries,
                            Err(e) => {
                                warn!("Failed to fetch memory context: {}", e);
                                Vec::new()
                            }
                        };

                        // Загружаем историю сообщений
                        let history = match session_repo.get_recent_messages(session_uuid, 20).await {
                            Ok(msgs) => msgs,
                            Err(e) => {
                                warn!("Failed to fetch message history: {}", e);
                                Vec::new()
                            }
                        };

                        // Собираем контекст через ContextBuilder
                        let system_prompt = format!(
                            "You are Sovalune, an AI assistant with long-term memory. \
                             You help users with software engineering tasks. \
                             Be helpful, accurate, and concise. \
                             Project ID: {}", project_id
                        );

                        let mut builder = ContextBuilder::new(128_000)
                            .with_system(&system_prompt);

                        // Добавляем секции памяти
                        for entry in &memory_entries {
                            builder = builder.with_memory_section(&entry.tier, &entry.content);
                        }

                        // Добавляем историю
                        for msg in &history {
                            builder = builder.with_history_entry(&msg.role, &msg.content);
                        }

                        // Добавляем пользовательский ввод
                        builder = builder.with_user_input(&content);

                        let context = builder.build();

                        // Создаём запрос на инференс
                        let inference_request = InferenceRequest {
                            id: inference_request_id,
                            session_id: session_id.clone(),
                            project_id: project_id.clone(),
                            context,
                            user_input: content,
                            config: GenerationConfig {
                                max_tokens: 4096,
                                temperature: 0.7,
                                top_p: 0.9,
                                stream: true,
                            },
                        };

                        // Запускаем инференс
                        let state_clone = state.clone();
                        let sender_clone = sender.clone();
                        let session_clone = session_id.clone();

                        tokio::spawn(async move {
                            match state_clone.inference.stream_infer(inference_request).await {
                                Ok(mut stream) => {
                                    let mut full_response = String::new();

                                    while let Some(event_result) = stream.next().await {
                                        match event_result {
                                            Ok(token) => {
                                                if token.finished {
                                                    // Сохраняем ответ ассистента
                                                    let msg_uuid = Uuid::new_v4();
                                                    let session_repo = SessionRepository::new(
                                                        state_clone.storage.pool().clone(),
                                                    );

                                                    if let Err(e) = session_repo
                                                        .create_message(CreateMessage {
                                                            session_id: Uuid::parse_str(&session_clone).unwrap(),
                                                            role: "assistant".to_string(),
                                                            content: full_response.clone(),
                                                            tool_call: None,
                                                            request_id: msg_uuid,
                                                        })
                                                        .await
                                                    {
                                                        error!("Failed to save assistant message: {}", e);
                                                    }

                                                    // Отправляем завершение
                                                    let complete = ServerMessage::MessageComplete {
                                                        session_id: session_clone.clone(),
                                                        message_id: msg_uuid.to_string(),
                                                    };
                                                    let mut s = sender_clone.lock().await;
                                                    let _ = s
                                                        .send(Message::Text(
                                                            serde_json::to_string(&complete).unwrap().into(),
                                                        ))
                                                        .await;
                                                    break;
                                                }

                                                // Стримим токен
                                                full_response.push_str(&token.delta);
                                                let token_msg = ServerMessage::Token {
                                                    session_id: session_clone.clone(),
                                                    delta: token.delta,
                                                };
                                                let mut s = sender_clone.lock().await;
                                                let _ = s
                                                    .send(Message::Text(
                                                        serde_json::to_string(&token_msg).unwrap().into(),
                                                    ))
                                                    .await;
                                            }
                                            Err(e) => {
                                                error!("Inference error: {}", e);
                                                send_error(&sender_clone, "INFERENCE_ERROR", &e.to_string()).await;
                                                break;
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to start inference: {}", e);
                                    send_error(&sender_clone, "INFERENCE_FAILED", &e.to_string()).await;
                                }
                            }
                        });
                    }

                    Ok(ClientMessage::JoinSession {
                        session_id,
                        project_id,
                    }) => {
                        info!("Joined session {} (project: {})", session_id, project_id);
                    }

                    Ok(ClientMessage::StopGeneration { session_id }) => {
                        info!("Stop generation requested for session {}", session_id);
                    }

                    Err(e) => {
                        error!("Failed to parse client message: {}", e);
                        send_error(&sender, "PARSE_ERROR", &format!("Invalid message format: {}", e)).await;
                    }
                }
            }
            Message::Close(_) => {
                info!("WebSocket closed: {}", request_id);
                break;
            }
            _ => {}
        }
    }
}

/// Отправляет сообщение об ошибке клиенту.
async fn send_error(sender: &Arc<Mutex<futures::stream::SplitSink<WebSocket, Message>>>, code: &str, message: &str) {
    let err = ServerMessage::Error {
        code: code.to_string(),
        message: message.to_string(),
    };
    let mut s = sender.lock().await;
    let _ = s
        .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
        .await;
}
