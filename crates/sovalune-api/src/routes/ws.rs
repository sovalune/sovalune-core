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
                        
                        // TODO: Send to NATS for inference
                        // For now, echo back
                        let response = ServerMessage::Token {
                            session_id: session_id.clone(),
                            delta: format!("Echo: {}", content),
                        };
                        
                        if let Ok(json) = serde_json::to_string(&response) {
                            let _ = sender.send(Message::Text(json.into())).await;
                        }
                        
                        // Send completion
                        let complete = ServerMessage::MessageComplete {
                            session_id,
                            message_id: Uuid::new_v4().to_string(),
                        };
                        
                        if let Ok(json) = serde_json::to_string(&complete) {
                            let _ = sender.send(Message::Text(json.into())).await;
                        }
                    }
                    Ok(ClientMessage::StopGeneration { session_id }) => {
                        info!("Stop generation for session {}", session_id);
                        // TODO: Cancel ongoing generation
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
