//! Локальный бэкенд для моделей.
//!
//! Подключается к локальному HTTP-серверу моделей (Ollama, llama.cpp, vLLM).
//! Использует тот же формат Chat API, что и OpenAI.
//!
//! # Поддерживаемые серверы
//!
//! - **Ollama** — `http://localhost:11434/v1/chat/completions`
//! - **llama.cpp** — `http://localhost:8080/v1/chat/completions`
//! - **vLLM** — `http://localhost:8000/v1/chat/completions`

use async_trait::async_trait;
use futures::stream::BoxStream;
use reqwest::Client;
use tokio::time::Duration;
use tracing::{debug, info};

use super::{BackendConfig, ModelBackend};
use crate::types::{InferenceError, InferenceRequest, TokenEvent};

use super::openai::OpenAIBackend;

/// Локальный бэкенд — обёртка над OpenAI-совместимым API.
///
/// Отличается от OpenAI-бэкенда тем, что:
/// - Не требует API-ключа
/// - Использует localhost по умолчанию
/// - Может автоматически определять параметры модели
pub struct LocalBackend {
    /// Внутренний OpenAI-совместимый бэкенд.
    inner: OpenAIBackend,
    /// URL сервера.
    server_url: String,
}

impl LocalBackend {
    /// Создаёт локальный бэкенд.
    pub fn new(server_url: &str, model: &str, timeout_secs: u64) -> Self {
        let inner = OpenAIBackend::new(
            &format!("{}/v1", server_url.trim_end_matches('/')),
            "no-key",
            model,
            timeout_secs,
        );

        Self {
            inner,
            server_url: server_url.to_string(),
        }
    }

    /// Автоматически определяет доступную модель на сервере.
    pub async fn detect_model(server_url: &str) -> Result<String, InferenceError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| InferenceError::Internal(e.to_string()))?;

        let models_url = format!("{}/api/tags", server_url);
        if let Ok(resp) = client.get(&models_url).send().await {
            if resp.status().is_success() {
                if let Ok(data) = resp.json::<serde_json::Value>().await {
                    if let Some(models) = data["models"].as_array() {
                        if let Some(first) = models.first() {
                            if let Some(name) = first["name"].as_str() {
                                info!("Detected local model: {}", name);
                                return Ok(name.to_string());
                            }
                        }
                    }
                }
            }
        }

        Ok("llama3".to_string())
    }

    /// Проверяет доступность сервера.
    pub async fn check_server(server_url: &str) -> bool {
        let client = Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap_or_default();

        let ollama_url = format!("{}/api/tags", server_url);
        if let Ok(resp) = client.get(&ollama_url).send().await {
            if resp.status().is_success() {
                return true;
            }
        }

        let openai_url = format!("{}/v1/models", server_url);
        if let Ok(resp) = client.get(&openai_url).send().await {
            if resp.status().is_success() {
                return true;
            }
        }

        false
    }
}

#[async_trait]
impl ModelBackend for LocalBackend {
    async fn stream_inference(
        &self,
        request: &InferenceRequest,
    ) -> Result<BoxStream<'_, Result<TokenEvent, InferenceError>>, InferenceError> {
        debug!("Local inference: model={}", self.inner.model_name());
        self.inner.stream_inference(request).await
    }

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }

    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }

    async fn health_check(&self) -> bool {
        Self::check_server(&self.server_url).await
    }

    async fn from_config(config: &BackendConfig) -> Result<Self, InferenceError> {
        let model = if config.model_name.is_empty() {
            Self::detect_model(&config.api_url).await?
        } else {
            config.model_name.clone()
        };

        Ok(Self::new(&config.api_url, &model, config.timeout_secs))
    }
}
