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
use sovalune_bus::{InferenceRequest, InferencePayload, PromptContext, GenerationConfig};
use sovalune_storage_client::{SessionRepository, MemoryRepository, CreateMessage, MemoryFilter};

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "user_message")]
    UserMessage {
        session_id: String,
        project_id: String,
        content: String,
    },
    #[serde(rename = "stop_generation")]
    StopGeneration {
        session_id: String,
    },
    #[serde(rename = "join_session")]
    JoinSession {
        session_id: String,
        project_id: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "token")]
    Token {
        session_id: String,
        delta: String,
    },
    #[serde(rename = "tool_call_started")]
    ToolCallStarted {
        session_id: String,
        tool: String,
        arguments: serde_json::Value,
    },
    #[serde(rename = "tool_call_finished")]
    ToolCallFinished {
        session_id: String,
        tool: String,
        result_summary: String,
    },
    #[serde(rename = "message_complete")]
    MessageComplete {
        session_id: String,
        message_id: String,
    },
    #[serde(rename = "learning_cycle_update")]
    LearningCycleUpdate {
        cycle_id: String,
        status: String,
        detail: serde_json::Value,
    },
    #[serde(rename = "error")]
    Error {
        code: String,
        message: String,
    },
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

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
                    Ok(ClientMessage::UserMessage { session_id, project_id, content }) => {
                        info!("User message in session {}: {}", session_id, &content[..50.min(content.len())]);
                        
                        let session_uuid = match Uuid::parse_str(&session_id) {
                            Ok(id) => id,
                            Err(e) => {
                                let err = ServerMessage::Error {
                                    code: "INVALID_SESSION".to_string(),
                                    message: format!("Invalid session ID: {}", e),
                                };
                                let mut s = sender.lock().await;
                                let _ = s.send(Message::Text(serde_json::to_string(&err).unwrap().into())).await;
                                continue;
                            }
                        };
                        
                        let project_uuid = match Uuid::parse_str(&project_id) {
                            Ok(id) => id,
                            Err(e) => {
                                let err = ServerMessage::Error {
                                    code: "INVALID_PROJECT".to_string(),
                                    message: format!("Invalid project ID: {}", e),
                                };
                                let mut s = sender.lock().await;
                                let _ = s.send(Message::Text(serde_json::to_string(&err).unwrap().into())).await;
                                continue;
                            }
                        };
                        
                        let session_repo = SessionRepository::new(state.storage.pool().clone());
                        let inference_request_id = Uuid::new_v4();
                        
                        match session_repo.create_message(CreateMessage {
                            session_id: session_uuid,
                            role: "user".to_string(),
                            content: content.clone(),
                            tool_call: None,
                            request_id: inference_request_id,
                        }).await {
                            Ok(_) => {}
                            Err(e) => {
                                error!("Failed to save user message: {}", e);
                                let err = ServerMessage::Error {
                                    code: "DB_ERROR".to_string(),
                                    message: format!("Failed to save message: {}", e),
                                };
                                let mut s = sender.lock().await;
                                let _ = s.send(Message::Text(serde_json::to_string(&err).unwrap().into())).await;
                                continue;
                            }
                        };
                        
                        let memory_repo = MemoryRepository::new(state.storage.pool().clone());
                        let memory_filter = MemoryFilter {
                            project_id: Some(project_uuid),
                            tier: None,
                            min_confidence: Some(0.5),
                            archived: Some(false),
                            query: Some(content.clone()),
                        };
                        
                        let memory_context = match memory_repo.list(memory_filter, 10, 0).await {
                            Ok(entries) => {
                                entries.into_iter().map(|e| {
                                    sovalune_bus::MemorySection {
                                        tier: e.tier,
                                        content: e.content,
                                    }
                                }).collect()
                            }
                            Err(e) => {
                                warn!("Failed to fetch memory context: {}", e);
                                Vec::new()
                            }
                        };
                        
                        let history = match session_repo.get_recent_messages(session_uuid, 20).await {
                            Ok(msgs) => {
                                msgs.into_iter().map(|m| {
                                    sovalune_bus::HistoryEntry {
                                        role: m.role,
                                        content: m.content,
                                    }
                                }).collect()
                            }
                            Err(e) => {
                                warn!("Failed to fetch message history: {}", e);
                                Vec::new()
                            }
                        };
                        
                        let system_prompt = format!(
                            "You are Sovalune, an AI assistant with long-term memory. \
                             You help users with software engineering tasks. \
                             Be helpful, accurate, and concise. \
                             Project: {}", project_id
                        );
                        
                        let inference_request = InferenceRequest {
                            request_id: inference_request_id.to_string(),
                            session_id: session_id.clone(),
                            project_id: project_id.clone(),
                            prompt_context: PromptContext {
                                system: system_prompt,
                                memory_sections: memory_context,
                                history,
                            },
                            generation_config: GenerationConfig {
                                max_tokens: 4096,
                                temperature: 0.7,
                            },
                        };
                        
                        if let Err(e) = state.nats.publish_inference_request(&inference_request).await {
                            error!("Failed to publish inference request: {}", e);
                            let err = ServerMessage::Error {
                                code: "INFERENCE_FAILED".to_string(),
                                message: format!("Failed to start inference: {}", e),
                            };
                            let mut s = sender.lock().await;
                            let _ = s.send(Message::Text(serde_json::to_string(&err).unwrap().into())).await;
                            continue;
                        }
                        
                        let state_clone = state.clone();
                        let sender_clone = sender.clone();
                        let session_clone = session_id.clone();
                        let inference_id = inference_request_id.to_string();
                        
                        let mut sub = match state.nats.client().subscribe(format!("inference.response.{}", inference_id)).await {
                            Ok(sub) => sub,
                            Err(e) => {
                                error!("Failed to subscribe to inference response: {}", e);
                                continue;
                            }
                        };
                        
                        let mut full_response = String::new();
                        
                        while let Some(msg) = sub.next().await {
                            if let Ok(response) = serde_json::from_slice::<sovalune_bus::InferenceResponse>(&msg.payload) {
                                match response.payload {
                                    InferencePayload::Delta { delta } => {
                                        full_response.push_str(&delta);
                                        let token_msg = ServerMessage::Token {
                                            session_id: session_clone.clone(),
                                            delta,
                                        };
                                        let mut s = sender_clone.lock().await;
                                        let _ = s.send(Message::Text(serde_json::to_string(&token_msg).unwrap().into())).await;
                                    }
                                    InferencePayload::Done { done: true, message_id } => {
                                        let session_repo = SessionRepository::new(state_clone.storage.pool().clone());
                                        let msg_uuid = Uuid::parse_str(&message_id).unwrap_or_else(|_| Uuid::new_v4());
                                        
                                        if let Err(e) = session_repo.create_message(CreateMessage {
                                            session_id: Uuid::parse_str(&session_clone).unwrap(),
                                            role: "assistant".to_string(),
                                            content: full_response.clone(),
                                            tool_call: None,
                                            request_id: msg_uuid,
                                        }).await {
                                            error!("Failed to save assistant message: {}", e);
                                        }
                                        
                                        let complete_msg = ServerMessage::MessageComplete {
                                            session_id: session_clone.clone(),
                                            message_id: msg_uuid.to_string(),
                                        };
                                        let mut s = sender_clone.lock().await;
                                        let _ = s.send(Message::Text(serde_json::to_string(&complete_msg).unwrap().into())).await;
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    
                    Ok(ClientMessage::JoinSession { session_id, project_id }) => {
                        info!("Joined session {} (project: {})", session_id, project_id);
                    }
                    
                    Ok(ClientMessage::StopGeneration { session_id }) => {
                        info!("Stop generation requested for session {}", session_id);
                    }
                    
                    Err(e) => {
                        error!("Failed to parse client message: {}", e);
                        let err = ServerMessage::Error {
                            code: "PARSE_ERROR".to_string(),
                            message: format!("Invalid message format: {}", e),
                        };
                        let mut s = sender.lock().await;
                        let _ = s.send(Message::Text(serde_json::to_string(&err).unwrap().into())).await;
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
