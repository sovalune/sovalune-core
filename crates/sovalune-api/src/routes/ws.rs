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
use tracing::{info, error};
use uuid::Uuid;

use crate::AppState;
use sovalune_bus::{InferenceRequest, InferencePayload, PromptContext, HistoryEntry, MemorySection, GenerationConfig};

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "user_message")]
    UserMessage {
        session_id: String,
        content: String,
    },
    #[serde(rename = "stop_generation")]
    StopGeneration {
        session_id: String,
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
    let (mut sender, mut receiver) = socket.split();
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
                    Ok(ClientMessage::UserMessage { session_id, content }) => {
                        info!("User message in session {}: {}", session_id, &content[..50.min(content.len())]);
                        
                        let inference_request_id = Uuid::new_v4().to_string();
                        
                        let request = InferenceRequest {
                            request_id: inference_request_id.clone(),
                            session_id: session_id.clone(),
                            prompt_context: PromptContext {
                                system: "You are Sovalune, an AI assistant. Be helpful, accurate, and concise.".to_string(),
                                memory_sections: Vec::new(),
                                history: Vec::new(),
                            },
                            generation_config: GenerationConfig {
                                max_tokens: 2048,
                                temperature: 0.7,
                            },
                        };
                        
                        if let Err(e) = state.nats.publish_inference_request(&request).await {
                            error!("Failed to publish inference request: {}", e);
                            let err = ServerMessage::Error {
                                code: "INFERENCE_FAILED".to_string(),
                                message: format!("Failed to start inference: {}", e),
                            };
                            if let Ok(json) = serde_json::to_string(&err) {
                                let _ = sender.send(Message::Text(json.into())).await;
                            }
                            continue;
                        }
                        
                        let request_id_clone = inference_request_id.clone();
                        let session_id_clone = session_id.clone();
                        let mut sender_clone = sender.clone();
                        
                        let state_clone = state.clone();
                        tokio::spawn(async move {
                            let mut response_count = 0;
                            
                            let request_id = request_id_clone;
                            let session_id = session_id_clone;
                            
                            if let Err(e) = state_clone.nats.subscribe_inference_response(
                                &request_id,
                                move |response| {
                                    response_count += 1;
                                    
                                    let msg = match response.payload {
                                        InferencePayload::Delta { delta } => {
                                            ServerMessage::Token {
                                                session_id: session_id.clone(),
                                                delta,
                                            }
                                        }
                                        InferencePayload::Done { message_id, .. } => {
                                            ServerMessage::MessageComplete {
                                                session_id: session_id.clone(),
                                                message_id,
                                            }
                                        }
                                    };
                                    
                                    if let Ok(json) = serde_json::to_string(&msg) {
                                        let _ = sender_clone.try_send(Message::Text(json.into()));
                                    }
                                }
                            ).await {
                                error!("Failed to subscribe to inference response: {}", e);
                            }
                        });
                    }
                    Ok(ClientMessage::StopGeneration { session_id }) => {
                        info!("Stop generation for session {}", session_id);
                    }
                    Err(e) => {
                        error!("Invalid message format: {}", e);
                        let err = ServerMessage::Error {
                            code: "INVALID_MESSAGE".to_string(),
                            message: e.to_string(),
                        };
                        if let Ok(json) = serde_json::to_string(&err) {
                            let _ = sender.send(Message::Text(json.into())).await;
                        }
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
    
    info!("WebSocket disconnected: {}", request_id);
}
