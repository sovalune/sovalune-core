//! Движок инференса — оркестратор генерации ответов.
//!
//! InferenceEngine связывает ContextBuilder, ModelBackend и хранилище,
//! обеспечивая полный pipeline: запрос → контекст → модель → ответ.
//!
//! # Архитектура
//!
//! ```text
//! InferenceRequest
//!     ↓
//! ContextBuilder → ContextSections
//!     ↓
//! ModelBackend → TokenStream
//!     ↓
//! InferenceResult
//! ```

use std::sync::Arc;
use futures::stream::BoxStream;
use futures::StreamExt;
use tokio::sync::RwLock;
use tracing::{debug, info, warn, error};
use uuid::Uuid;

use crate::backend::{BackendConfig, ModelBackend};
use crate::context::ContextBuilder;
use crate::types::{
    GenerationConfig, InferenceError, InferenceRequest, InferenceResult, TokenEvent,
};

/// Движок инференса — центральный компонент для генерации ответов.
///
/// Управляет бэкендом моделей, собирает контекст и стримит ответы.
///
/// # Пример
///
/// ```rust,ignore
/// let engine = InferenceEngine::new(backend);
///
/// let result = engine.infer(InferenceRequest {
///     id: Uuid::new_v4(),
///     session_id: "session-1".into(),
///     project_id: "project-1".into(),
///     context: vec![],
///     user_input: "Hello!".into(),
///     config: GenerationConfig::default(),
/// }).await?;
/// ```
pub struct InferenceEngine {
    /// Бэкенд моделей.
    backend: Arc<dyn ModelBackend>,
    /// Конфигурация по умолчанию.
    default_config: GenerationConfig,
}

impl InferenceEngine {
    /// Создаёт новый движок с указанным бэкендом.
    pub fn new(backend: Arc<dyn ModelBackend>) -> Self {
        Self {
            backend,
            default_config: GenerationConfig::default(),
        }
    }

    /// Создаёт движок из конфигурации.
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
    /// Возвращает стрим `TokenEvent` — каждый элемент содержит один токен.
    pub async fn stream_infer(
        &self,
        mut request: InferenceRequest,
    ) -> Result<BoxStream<'_, Result<TokenEvent, InferenceError>>, InferenceError> {
        // Проверяем здоровье бэкенда
        if !self.backend.health_check().await {
            return Err(InferenceError::BackendUnavailable(
                "Backend health check failed".into(),
            ));
        }

        // Проверяем лимит контекста
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

        // Запускаем стриминг
        let mut token_stream = self.backend.stream_inference(&request).await?;

        // Оборачиваем стрим для логирования и метрик
        let request_id = request.id;
        let model_name = self.backend.model_name().to_string();

        let wrapped_stream = async_stream::stream! {
            let mut tokens_generated = 0u32;
            let mut content = String::new();

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

    /// Запускает инференс и собирает полный ответ (не-стриминговый режим).
    pub async fn infer(
        &self,
        request: InferenceRequest,
    ) -> Result<InferenceResult, InferenceError> {
        let request_id = request.id;
        let start = std::time::Instant::now();

        let mut stream = self.stream_infer(request).await?;
        let mut content = String::new();
        let mut tokens_used = 0u32;

        while let Some(event) = stream.next().await {
            match event {
                Ok(token) => {
                    if token.finished {
                        break;
                    }
                    content.push_str(&token.delta);
                    tokens_used += 1;
                }
                Err(e) => return Err(e),
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(InferenceResult {
            request_id,
            content,
            tokens_used,
            model: self.backend.model_name().to_string(),
            duration_ms,
        })
    }

    /// Возвращает информацию о бэкенде.
    pub fn backend_info(&self) -> BackendInfo {
        BackendInfo {
            model: self.backend.model_name().to_string(),
            max_context_tokens: self.backend.max_context_tokens(),
        }
    }

    /// Проверяет здоровье бэкенда.
    pub async fn health_check(&self) -> bool {
        self.backend.health_check().await
    }
}

/// Информация о бэкенде.
#[derive(Debug, Clone)]
pub struct BackendInfo {
    /// Название модели.
    pub model: String,
    /// Максимальный контекст в токенах.
    pub max_context_tokens: usize,
}

/// Фабрика движков инференса.
///
/// Управляет созданием и кешированием движков для разных проектов.
pub struct InferenceEngineFactory {
    /// Конфигурации бэкендов по проектам.
    configs: RwLock<std::collections::HashMap<String, BackendConfig>>,
}

impl InferenceEngineFactory {
    /// Создаёт новую фабрику.
    pub fn new() -> Self {
        Self {
            configs: RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Регистрирует конфигурацию бэкенда для проекта.
    pub async fn register_project(
        &self,
        project_id: &str,
        config: BackendConfig,
    ) {
        let mut configs = self.configs.write().await;
        configs.insert(project_id.to_string(), config);
    }

    /// Создаёт движок для проекта.
    pub async fn create_engine(
        &self,
        project_id: &str,
    ) -> Result<InferenceEngine, InferenceError> {
        let configs = self.configs.read().await;
        let config = configs
            .get(project_id)
            .cloned()
            .unwrap_or_default();

        InferenceEngine::from_config(&config).await
    }

    /// Создаёт движок с конфигурацией по умолчанию.
    pub async fn create_default_engine(&self) -> Result<InferenceEngine, InferenceError> {
        InferenceEngine::from_config(&BackendConfig::default()).await
    }
}

impl Default for InferenceEngineFactory {
    fn default() -> Self {
        Self::new()
    }
}
