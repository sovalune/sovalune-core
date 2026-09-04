//! Движок инференса — оркестратор генерации ответов.
//!
//! InferenceEngine связывает ContextBuilder, ModelBackend и хранилище,
//! обеспечивая полный pipeline: запрос → контекст → модель → ответ.
//!
//! Включает:
//! - Кеширование ответов (LRU с TTL)
//! - Retry с экспоненциальной задержкой
//! - Автоматический выбор retry-стратегии по типу ошибки

use futures::stream::BoxStream;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::backend::{BackendConfig, ModelBackend};
use crate::cache::{model_cache_key, Cache, CacheFactory};
use crate::retry::{extract_retry_after, should_retry, RetryConfig, RetryState};
use crate::types::{InferenceError, InferenceRequest, InferenceResult, TokenEvent};

/// Движок инференса — центральный компонент для генерации ответов.
///
/// Управляет бэкендом моделей, собирает контекст, стримит ответы,
/// кеширует результаты и автоматически повторяет при ошибках.
pub struct InferenceEngine {
    backend: Arc<dyn ModelBackend>,
    response_cache: Arc<Cache<String, String>>,
}

impl InferenceEngine {
    pub fn new(backend: Arc<dyn ModelBackend>) -> Self {
        Self {
            backend,
            response_cache: Arc::new(CacheFactory::response_cache()),
        }
    }

    pub fn with_cache(
        backend: Arc<dyn ModelBackend>,
        response_cache: Arc<Cache<String, String>>,
    ) -> Self {
        Self {
            backend,
            response_cache,
        }
    }

    pub async fn from_config(config: &BackendConfig) -> Result<Self, InferenceError> {
        let backend: Arc<dyn ModelBackend> = match config.backend_type.as_str() {
            "openai" => Arc::new(crate::backend::OpenAIBackend::from_config(config).await?),
            "local" => Arc::new(crate::backend::LocalBackend::from_config(config).await?),
            other => {
                return Err(InferenceError::Internal(format!(
                    "Unknown backend type: {}",
                    other
                )))
            }
        };

        Ok(Self::new(backend))
    }

    /// Запускает полный инференс с потоковой передачей токенов.
    ///
    /// Включает retry с экспоненциальной задержкой при ошибках.
    pub async fn stream_infer(
        &self,
        request: InferenceRequest,
    ) -> Result<BoxStream<'_, Result<TokenEvent, InferenceError>>, InferenceError> {
        if !self.backend.health_check().await {
            return Err(InferenceError::BackendUnavailable(
                "Backend health check failed".into(),
            ));
        }

        let total_tokens: usize = request.context.iter().map(|s| s.token_estimate).sum();
        let max_tokens = self.backend.max_context_tokens();
        if total_tokens > max_tokens {
            warn!(
                "Context too long: {} tokens (max {}), will be truncated",
                total_tokens, max_tokens
            );
        }

        info!(
            "Starting inference: request_id={}, model={}, tokens_estimate={}",
            request.id,
            self.backend.model_name(),
            total_tokens
        );

        let start = std::time::Instant::now();
        let mut retry_state = RetryState::new(RetryConfig::default());

        // Retry loop for stream_infer
        let token_stream = loop {
            match self.backend.stream_inference(&request).await {
                Ok(stream) => break stream,
                Err(e) => {
                    let error_str = format!("{}", e);
                    if !retry_state.has_retries_left() || !should_retry(&error_str) {
                        return Err(e);
                    }
                    let delay =
                        extract_retry_after(&error_str).unwrap_or_else(|| retry_state.increment());
                    warn!(
                        "Stream failed (attempt {}), retrying in {:?}: {}",
                        retry_state.attempt(),
                        delay,
                        &error_str[..100.min(error_str.len())]
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        };

        let request_id = request.id;

        let wrapped_stream = async_stream::stream! {
            let mut tokens_generated = 0u32;
            let mut content = String::new();

            let mut token_stream = token_stream;
            while let Some(event) = token_stream.next().await {
                match event {
                    Ok(token) => {
                        if token.finished {
                            let duration = start.elapsed().as_millis() as u64;
                            info!(
                                "Inference complete: request_id={}, tokens={}, duration={}ms",
                                request_id, tokens_generated, duration
                            );

                            yield Ok(TokenEvent {
                                request_id,
                                delta: String::new(),
                                finished: true,
                                message_id: Some(Uuid::new_v4().to_string()),
                            });
                        } else {
                            content.push_str(&token.delta);
                            tokens_generated += 1;
                            yield Ok(token);
                        }
                    }
                    Err(e) => {
                        error!("Inference error: request_id={}, error={}", request_id, e);
                        yield Err(e);
                    }
                }
            }
        };

        Ok(Box::pin(wrapped_stream))
    }

    /// Запускает инференс и собирает полный ответ.
    ///
    /// Кеширует результат по ключу (model + messages + temperature + max_tokens).
    pub async fn infer(
        &self,
        request: InferenceRequest,
    ) -> Result<InferenceResult, InferenceError> {
        // Build cache key from request
        let messages: Vec<serde_json::Value> = request
            .context
            .iter()
            .map(|s| {
                serde_json::json!({
                    "role": s.role,
                    "content": s.content
                })
            })
            .chain(std::iter::once(serde_json::json!({
                "role": "user",
                "content": request.user_input
            })))
            .collect();

        let cache_key = model_cache_key(
            self.backend.model_name(),
            &messages,
            request.config.temperature,
            request.config.max_tokens,
        );

        // Check cache
        if let Some(cached) = self.response_cache.get(&cache_key).await {
            debug!("Cache hit for request_id={}", request.id);
            let tokens_used = self.backend.estimate_tokens(&cached);
            return Ok(InferenceResult {
                request_id: request.id,
                content: cached,
                tokens_used,
                model: self.backend.model_name().to_string(),
                duration_ms: 0,
            });
        }

        // Retry loop
        let mut retry_state = RetryState::new(RetryConfig::default());

        let result = loop {
            let request_id = request.id;
            let start = std::time::Instant::now();

            match self.backend.stream_inference(&request).await {
                Ok(mut stream) => {
                    let mut content = String::new();
                    let mut tokens_used = 0u32;
                    let mut has_error = false;

                    while let Some(event) = stream.next().await {
                        match event {
                            Ok(token) => {
                                if token.finished {
                                    break;
                                }
                                content.push_str(&token.delta);
                                tokens_used += 1;
                            }
                            Err(e) => {
                                let error_str = format!("{}", e);
                                if retry_state.has_retries_left() && should_retry(&error_str) {
                                    let delay = extract_retry_after(&error_str)
                                        .unwrap_or_else(|| retry_state.increment());
                                    warn!(
                                        "Infer failed (attempt {}), retrying in {:?}",
                                        retry_state.attempt(),
                                        delay
                                    );
                                    tokio::time::sleep(delay).await;
                                    has_error = true;
                                    break;
                                }
                                return Err(e);
                            }
                        }
                    }

                    if has_error {
                        continue;
                    }

                    let duration_ms = start.elapsed().as_millis() as u64;
                    let result = InferenceResult {
                        request_id,
                        content,
                        tokens_used,
                        model: self.backend.model_name().to_string(),
                        duration_ms,
                    };

                    // Cache the result
                    self.response_cache
                        .insert(cache_key.clone(), result.content.clone())
                        .await;

                    break result;
                }
                Err(e) => {
                    let error_str = format!("{}", e);
                    if retry_state.has_retries_left() && should_retry(&error_str) {
                        let delay = extract_retry_after(&error_str)
                            .unwrap_or_else(|| retry_state.increment());
                        warn!(
                            "Infer failed (attempt {}), retrying in {:?}",
                            retry_state.attempt(),
                            delay
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(e);
                }
            }
        };

        Ok(result)
    }

    pub fn backend_info(&self) -> BackendInfo {
        BackendInfo {
            model: self.backend.model_name().to_string(),
            max_context_tokens: self.backend.max_context_tokens(),
        }
    }

    pub async fn health_check(&self) -> bool {
        self.backend.health_check().await
    }
}

/// Информация о бэкенде.
#[derive(Debug, Clone)]
pub struct BackendInfo {
    pub model: String,
    pub max_context_tokens: usize,
}

/// Фабрика движков инференса.
pub struct InferenceEngineFactory {
    configs: RwLock<std::collections::HashMap<String, BackendConfig>>,
}

impl InferenceEngineFactory {
    pub fn new() -> Self {
        Self {
            configs: RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub async fn register_project(&self, project_id: &str, config: BackendConfig) {
        let mut configs = self.configs.write().await;
        configs.insert(project_id.to_string(), config);
    }

    pub async fn create_engine(&self, project_id: &str) -> Result<InferenceEngine, InferenceError> {
        let configs = self.configs.read().await;
        let config = configs.get(project_id).cloned().unwrap_or_default();
        InferenceEngine::from_config(&config).await
    }

    pub async fn create_default_engine(&self) -> Result<InferenceEngine, InferenceError> {
        InferenceEngine::from_config(&BackendConfig::default()).await
    }
}

impl Default for InferenceEngineFactory {
    fn default() -> Self {
        Self::new()
    }
}
