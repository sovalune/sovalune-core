//! WebSocket handler for chat.
//!
//! Handles client WebSocket connections:
//! - Saves user messages to DB
//! - Loads memory context and history
//! - Builds prompt via ContextBuilder
//! - Runs inference via InferenceEngine
//! - Executes tool calls in a loop
//! - Streams tokens back to client
//! - Saves assistant response to DB

use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;
use sovalune_model_runtime::{
    ContextBuilder, GenerationConfig, InferenceRequest, ToolCallParser, ToolResult,
};
use sovalune_storage_client::{CreateMessage, MemoryFilter, SessionRepository};

/// Max tool call iterations to prevent infinite loops.
const MAX_TOOL_ITERATIONS: usize = 10;

/// Active generation cancel tokens keyed by session_id.
type CancelTokenMap = Arc<Mutex<HashMap<String, CancellationToken>>>;

/// Client message types.
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
    StopGeneration { session_id: String },
    #[serde(rename = "join_session")]
    JoinSession {
        session_id: String,
        project_id: String,
    },
}

/// Server message types.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "token")]
    Token { session_id: String, delta: String },
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
    Error { code: String, message: String },
}

type Sender = Arc<Mutex<futures::stream::SplitSink<WebSocket, Message>>>;

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
    let cancel_tokens: CancelTokenMap = Arc::new(Mutex::new(HashMap::new()));

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

                        let session_uuid = match Uuid::parse_str(&session_id) {
                            Ok(id) => id,
                            Err(e) => {
                                send_error(
                                    &sender,
                                    "INVALID_SESSION",
                                    &format!("Invalid session ID: {}", e),
                                )
                                .await;
                                continue;
                            }
                        };

                        let project_uuid = match Uuid::parse_str(&project_id) {
                            Ok(id) => id,
                            Err(e) => {
                                send_error(
                                    &sender,
                                    "INVALID_PROJECT",
                                    &format!("Invalid project ID: {}", e),
                                )
                                .await;
                                continue;
                            }
                        };

                        let inference_request_id = Uuid::new_v4();
                        let session_repo = SessionRepository::new(state.storage.pool().clone());

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
                            send_error(
                                &sender,
                                "DB_ERROR",
                                &format!("Failed to save message: {}", e),
                            )
                            .await;
                            continue;
                        }

                        // Search vector memory
                        let memory_filter = MemoryFilter {
                            project_id: Some(project_uuid),
                            tier: None,
                            min_confidence: Some(0.5),
                            archived: Some(false),
                            query: Some(content.clone()),
                        };

                        let memory_entries = match state
                            .vector_memory
                            .search_by_text_with_embedding(&content, memory_filter, 10)
                            .await
                        {
                            Ok(scored) => {
                                info!("Found {} relevant memory entries", scored.len());
                                scored.into_iter().map(|sm| sm.entry).collect()
                            }
                            Err(e) => {
                                warn!("Failed to search memory with embedding: {}", e);
                                let memory_repo = sovalune_storage_client::MemoryRepository::new(
                                    state.storage.pool().clone(),
                                );
                                let fallback_filter = MemoryFilter {
                                    project_id: Some(project_uuid),
                                    tier: None,
                                    min_confidence: Some(0.5),
                                    archived: Some(false),
                                    query: Some(content.clone()),
                                };
                                memory_repo
                                    .list(fallback_filter, 10, 0)
                                    .await
                                    .unwrap_or_default()
                            }
                        };

                        // Load message history
                        let history = match session_repo.get_recent_messages(session_uuid, 20).await
                        {
                            Ok(msgs) => msgs,
                            Err(e) => {
                                warn!("Failed to fetch message history: {}", e);
                                Vec::new()
                            }
                        };

                        // Build system prompt with tool descriptions
                        let tool_defs = state.tool_registry.definitions();
                        let tools_desc = if tool_defs.is_empty() {
                            String::new()
                        } else {
                            let defs: Vec<String> = tool_defs
                                .iter()
                                .map(|d| format!("- {}: {}", d.name, d.description))
                                .collect();
                            format!("\n\nYou have access to these tools:\n{}", defs.join("\n"))
                        };

                        let system_prompt = format!(
                            "You are Sovalune, an AI assistant with long-term memory. \
                             You help users with software engineering tasks. \
                             Be helpful, accurate, and concise. \
                             You can use tools to search memory, write memory, execute code, search the web, and run tests. \
                             When you need to use a tool, respond with a JSON tool call in a code block.\n\
                             Format: ```json\n{{\"name\": \"tool_name\", \"arguments\": {{...}}}}\n```\
                             {}\n\nProject ID: {}",
                            tools_desc, project_id
                        );

                        let mut builder = ContextBuilder::new(128_000).with_system(&system_prompt);

                        for entry in &memory_entries {
                            builder = builder.with_memory_section(&entry.tier, &entry.content);
                        }

                        for msg in &history {
                            builder = builder.with_history_entry(&msg.role, &msg.content);
                        }

                        builder = builder.with_user_input(&content);

                        // Set up cancellation
                        let cancel_token = CancellationToken::new();
                        {
                            let mut tokens = cancel_tokens.lock().await;
                            tokens.insert(session_id.clone(), cancel_token.clone());
                        }

                        let state_clone = state.clone();
                        let sender_clone = sender.clone();
                        let session_clone = session_id.clone();
                        let cancel_tokens_clone = cancel_tokens.clone();

                        tokio::spawn(async move {
                            let mut full_response = String::new();
                            let mut current_context = builder.build();
                            let mut iteration = 0;

                            loop {
                                if cancel_token.is_cancelled() {
                                    info!("Generation cancelled for session {}", session_clone);
                                    break;
                                }

                                iteration += 1;
                                if iteration > MAX_TOOL_ITERATIONS {
                                    warn!(
                                        "Max tool iterations reached for session {}",
                                        session_clone
                                    );
                                    break;
                                }

                                let inference_request = InferenceRequest {
                                    id: Uuid::new_v4(),
                                    session_id: session_clone.clone(),
                                    project_id: project_id.clone(),
                                    context: current_context.clone(),
                                    user_input: String::new(),
                                    config: GenerationConfig {
                                        max_tokens: 4096,
                                        temperature: 0.7,
                                        top_p: 0.9,
                                        stream: true,
                                    },
                                };

                                match state_clone.inference.stream_infer(inference_request).await {
                                    Ok(mut stream) => {
                                        let mut response_chunk = String::new();

                                        while let Some(event_result) = stream.next().await {
                                            if cancel_token.is_cancelled() {
                                                break;
                                            }

                                            match event_result {
                                                Ok(token) => {
                                                    if token.finished {
                                                        break;
                                                    }
                                                    response_chunk.push_str(&token.delta);
                                                    full_response.push_str(&token.delta);

                                                    let token_msg = ServerMessage::Token {
                                                        session_id: session_clone.clone(),
                                                        delta: token.delta,
                                                    };
                                                    let mut s = sender_clone.lock().await;
                                                    let _ = s
                                                        .send(Message::Text(
                                                            serde_json::to_string(&token_msg)
                                                                .unwrap()
                                                                .into(),
                                                        ))
                                                        .await;
                                                }
                                                Err(e) => {
                                                    error!("Inference error: {}", e);
                                                    send_error(
                                                        &sender_clone,
                                                        "INFERENCE_ERROR",
                                                        &e.to_string(),
                                                    )
                                                    .await;
                                                    return;
                                                }
                                            }
                                        }

                                        // Check for tool calls in the response
                                        let tool_calls =
                                            ToolCallParser::parse_text_tool_calls(&response_chunk);

                                        if tool_calls.is_empty() {
                                            // No tool calls — we're done
                                            break;
                                        }

                                        // Execute tool calls
                                        info!("Found {} tool calls in response", tool_calls.len());

                                        let mut tool_results: Vec<ToolResult> = Vec::new();
                                        for tc in &tool_calls {
                                            let started = ServerMessage::ToolCallStarted {
                                                session_id: session_clone.clone(),
                                                tool: tc.name.clone(),
                                                arguments: tc.arguments.clone(),
                                            };
                                            let mut s = sender_clone.lock().await;
                                            let _ = s
                                                .send(Message::Text(
                                                    serde_json::to_string(&started).unwrap().into(),
                                                ))
                                                .await;
                                            drop(s);

                                            match state_clone
                                                .tool_registry
                                                .execute(tc, "ws_user")
                                                .await
                                            {
                                                Ok(result) => {
                                                    let finished =
                                                        ServerMessage::ToolCallFinished {
                                                            session_id: session_clone.clone(),
                                                            tool: tc.name.clone(),
                                                            result_summary: format!(
                                                                "success={}, output={}",
                                                                result.success,
                                                                &result.output.to_string()[..100
                                                                    .min(
                                                                        result
                                                                            .output
                                                                            .to_string()
                                                                            .len()
                                                                    )]
                                                            ),
                                                        };
                                                    let mut s = sender_clone.lock().await;
                                                    let _ = s
                                                        .send(Message::Text(
                                                            serde_json::to_string(&finished)
                                                                .unwrap()
                                                                .into(),
                                                        ))
                                                        .await;
                                                    tool_results.push(result);
                                                }
                                                Err(e) => {
                                                    warn!("Tool call failed: {} - {}", tc.name, e);
                                                    tool_results.push(ToolResult {
                                                        call_id: tc.id.clone(),
                                                        tool_name: tc.name.clone(),
                                                        success: false,
                                                        output: serde_json::json!({
                                                            "error": e.to_string()
                                                        }),
                                                        duration_ms: 0,
                                                        side_effects: vec![],
                                                    });
                                                }
                                            }
                                        }

                                        // Build new context with tool results
                                        let mut tool_section =
                                            String::from("\n\n## Tool Results\n");
                                        for tr in &tool_results {
                                            tool_section.push_str(&format!(
                                                "\n### {} (call_id: {})\n{}\n",
                                                tr.tool_name,
                                                tr.call_id,
                                                serde_json::to_string_pretty(&tr.output)
                                                    .unwrap_or_default()
                                            ));
                                        }

                                        // Rebuild context with tool results
                                        let mut builder = ContextBuilder::new(128_000)
                                            .with_system(&system_prompt);
                                        for entry in &memory_entries {
                                            builder = builder
                                                .with_memory_section(&entry.tier, &entry.content);
                                        }
                                        for msg in &history {
                                            builder =
                                                builder.with_history_entry(&msg.role, &msg.content);
                                        }
                                        builder = builder.with_user_input(&format!(
                                            "{}\n\nUser original request: {}{}",
                                            tool_section, content, full_response
                                        ));
                                        current_context = builder.build();
                                    }
                                    Err(e) => {
                                        error!("Failed to start inference: {}", e);
                                        send_error(
                                            &sender_clone,
                                            "INFERENCE_FAILED",
                                            &e.to_string(),
                                        )
                                        .await;
                                        return;
                                    }
                                }
                            }

                            // Save final assistant response
                            let msg_uuid = Uuid::new_v4();
                            let session_repo =
                                SessionRepository::new(state_clone.storage.pool().clone());

                            if let Err(e) = session_repo
                                .create_message(CreateMessage {
                                    session_id: Uuid::parse_str(&session_clone).unwrap(),
                                    role: "assistant".to_string(),
                                    content: full_response,
                                    tool_call: None,
                                    request_id: msg_uuid,
                                })
                                .await
                            {
                                error!("Failed to save assistant message: {}", e);
                            }

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

                            // Clean up cancel token
                            let mut tokens = cancel_tokens_clone.lock().await;
                            tokens.remove(&session_clone);
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
                        let tokens = cancel_tokens.lock().await;
                        if let Some(token) = tokens.get(&session_id) {
                            token.cancel();
                            info!("Cancelled generation for session {}", session_id);
                        }
                    }

                    Err(e) => {
                        error!("Failed to parse client message: {}", e);
                        send_error(
                            &sender,
                            "PARSE_ERROR",
                            &format!("Invalid message format: {}", e),
                        )
                        .await;
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

async fn send_error(sender: &Sender, code: &str, message: &str) {
    let err = ServerMessage::Error {
        code: code.to_string(),
        message: message.to_string(),
    };
    let mut s = sender.lock().await;
    let _ = s
        .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
        .await;
}
