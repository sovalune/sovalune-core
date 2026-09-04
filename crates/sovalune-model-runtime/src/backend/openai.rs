//! Бэкенд для OpenAI-compatible API.
//!
//! Поддерживает любой API, совместимый с OpenAI (OpenAI, Together, Groq, etc.).
//! Использует Server-Sent Events (SSE) для потоковой передачи токенов.
//!
//! # Протокол
//!
//! Запрос отправляется POST на `/chat/completions` с `stream: true`.
//! Ответ — SSE-поток с событиями `data: {"choices": [{"delta": {"content": "..."}}]}`.
//! Завершение — событие `data: [DONE]`.

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::time::{timeout, Duration};
use tracing::{debug, warn};

use super::{BackendConfig, ModelBackend};
use crate::types::{InferenceRequest, TokenEvent, InferenceError};

/// Сообщение в формате OpenAI Chat API.
#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

/// Тело запроса к OpenAI Chat Completions API.
#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
    temperature: f32,
    top_p: f32,
    stream: bool,
}

/// SSE-событие из потока OpenAI.
#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

/// Бэкенд OpenAI-compatible API.
pub struct OpenAIBackend {
    /// HTTP-клиент для запросов.
    client: Client,
    /// Базовый URL API.
    api_url: String,
    /// API ключ.
    api_key: String,
    /// Имя модели.
    model: String,
    /// Таймаут запроса.
    timeout: Duration,
}

impl OpenAIBackend {
    /// Создаёт новый бэкенд с указанными параметрами.
    pub fn new(api_url: &str, api_key: &str, model: &str, timeout_secs: u64) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs + 30))
            .pool_max_idle_per_host(10)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            api_url: api_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    /// Формирует HTTP-запрос к Chat Completions API.
    fn build_request(
        &self,
        request: &InferenceRequest,
    ) -> Result<ChatCompletionRequest, InferenceError> {
        let mut messages = Vec::new();

        // Добавляем системный промпт и контекст
        for section in &request.context {
            match section.role.as_str() {
                "system" => {
                    messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: section.content.clone(),
                    });
                }
                "memory" | "history" => {
                    messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: section.content.clone(),
                    });
                }
                _ => {}
            }
        }

        // Добавляем пользовательский ввод
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: request.user_input.clone(),
        });

        Ok(ChatCompletionRequest {
            model: self.model.clone(),
            messages,
            max_tokens: request.config.max_tokens,
            temperature: request.config.temperature,
            top_p: request.config.top_p,
            stream: true,
        })
    }

    /// Парсит SSE-событие из строки.
    fn parse_sse_line(line: &str, request_id: uuid::Uuid) -> Vec<Result<TokenEvent, InferenceError>> {
        let line = line.trim();

        if line.is_empty() || line.starts_with(':') {
            return Vec::new();
        }

        if let Some(data) = line.strip_prefix("data: ") {
            let data = data.trim();

            if data == "[DONE]" {
                return vec![Ok(TokenEvent {
                    request_id,
                    delta: String::new(),
                    finished: true,
                    message_id: None,
                })];
            }

            if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                let mut events = Vec::new();
                for choice in chunk.choices {
                    if let Some(content) = choice.delta.content {
                        events.push(Ok(TokenEvent {
                            request_id,
                            delta: content,
                            finished: false,
                            message_id: None,
                        }));
                    }

                    if choice.finish_reason.is_some() {
                        events.push(Ok(TokenEvent {
                            request_id,
                            delta: String::new(),
                            finished: true,
                            message_id: None,
                        }));
                    }
                }
                return events;
            }
        }

        Vec::new()
    }
}

#[async_trait]
impl ModelBackend for OpenAIBackend {
    async fn stream_inference(
        &self,
        request: &InferenceRequest,
    ) -> Result<BoxStream<'_, Result<TokenEvent, InferenceError>>, InferenceError> {
        let req = self.build_request(request)?;
        let url = format!("{}/chat/completions", self.api_url);

        debug!(
            "Sending inference request to {} model={}",
            self.api_url, self.model
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&req)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    InferenceError::Timeout(self.timeout.as_millis() as u64)
                } else if e.is_connect() {
                    InferenceError::BackendUnavailable(e.to_string())
                } else {
                    InferenceError::Http(e)
                }
            })?;

        // Обработка ошибок API
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();

            if status == 429 {
                return Err(InferenceError::RateLimited {
                    retry_after_ms: 1000,
                });
            }

            return Err(InferenceError::ModelError(format!(
                "API error {}: {}",
                status, body
            )));
        }

        // Парсинг SSE-потока
        let request_id = request.id;
        let byte_stream = response.bytes_stream();

        let token_stream = byte_stream.filter_map(move |chunk| {
            let request_id = request_id;
            async move {
                match chunk {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        let mut events = Vec::new();

                        for line in text.lines() {
                            let parsed = Self::parse_sse_line(line, request_id);
                            events.extend(parsed);
                        }

                        if events.is_empty() {
                            None
                        } else {
                            Some(futures::stream::iter(events))
                        }
                    }
                    Err(e) => {
                        warn!("Stream error: {}", e);
                        Some(futures::stream::iter(vec![Err(InferenceError::Http(e))]))
                    }
                }
            }
        })
        .flatten();

        Ok(Box::pin(token_stream))
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn max_context_tokens(&self) -> usize {
        match self.model.as_str() {
            m if m.contains("gpt-4o") => 128_000,
            m if m.contains("gpt-4") => 8_192,
            m if m.contains("gpt-3.5") => 16_385,
            m if m.contains("llama-3") => 8_192,
            m if m.contains("mixtral") => 32_768,
            _ => 4_096,
        }
    }

    async fn health_check(&self) -> bool {
        let url = format!("{}/models", self.api_url);
        let result = timeout(
            Duration::from_secs(5),
            self.client
                .get(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .send(),
        )
        .await;

        match result {
            Ok(Ok(resp)) => resp.status().is_success(),
            _ => false,
        }
    }

    async fn from_config(config: &BackendConfig) -> Result<Self, InferenceError> {
        let api_key = config
            .api_key
            .as_deref()
            .ok_or_else(|| InferenceError::Internal("API key required for OpenAI backend".into()))?;

        Ok(Self::new(
            &config.api_url,
            api_key,
            &config.model_name,
            config.timeout_secs,
        ))
    }
}
