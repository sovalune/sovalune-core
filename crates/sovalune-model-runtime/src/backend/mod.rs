//! Модуль бэкендов моделей.
//!
//! Содержит трейт `ModelBackend` — абстракцию над провайдером моделей,
//! и конкретные реализации: OpenAI-compatible API и локальный HTTP-сервер.

pub mod openai;
pub mod local;

pub use openai::OpenAIBackend;
pub use local::LocalBackend;

use async_trait::async_trait;
use futures::stream::BoxStream;
use crate::types::{InferenceRequest, TokenEvent, InferenceError};

/// Конфигурация бэкенда моделей.
#[derive(Debug, Clone)]
pub struct BackendConfig {
    /// Тип бэкенда: "openai", "local".
    pub backend_type: String,
    /// URL API (для OpenAI: https://api.openai.com/v1, для local: http://localhost:11434).
    pub api_url: String,
    /// API ключ (для OpenAI).
    pub api_key: Option<String>,
    /// Имя модели (для OpenAI: gpt-4, для local: llama3).
    pub model_name: String,
    /// Таймаут запроса (сек).
    pub timeout_secs: u64,
    /// Максимальное количество повторных попыток.
    pub max_retries: u32,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            backend_type: "openai".to_string(),
            api_url: "https://api.openai.com/v1".to_string(),
            api_key: None,
            model_name: "gpt-4".to_string(),
            timeout_secs: 120,
            max_retries: 3,
        }
    }
}

/// Трейт бэкенда моделей — абстракция над любым провайдером инференса.
///
/// Каждый бэкенд реализует метод `stream_inference`, который возвращает
/// асинхронный стрим токенов. Это позволяет работать с моделями в реальном времени.
///
/// # Пример реализации
///
/// ```rust,ignore
/// pub struct MyBackend;
///
/// #[async_trait]
/// impl ModelBackend for MyBackend {
///     async fn stream_inference(
///         &self,
///         request: &InferenceRequest,
///     ) -> Result<BoxStream<'_, Result<TokenEvent, InferenceError>>, InferenceError> {
///         // Реализация стриминга
///     }
///
///     fn model_name(&self) -> &str { "my-model" }
///     fn max_context_tokens(&self) -> usize { 4096 }
/// }
/// ```
#[async_trait]
pub trait ModelBackend: Send + Sync {
    /// Запускает стриминговый инференс и возвращает стрим токенов.
    ///
    /// Каждый элемент стрима — это `TokenEvent` с одним токеном (словом/символом).
    /// При `finished: true` стрим завершается.
    async fn stream_inference(
        &self,
        request: &InferenceRequest,
    ) -> Result<BoxStream<'_, Result<TokenEvent, InferenceError>>, InferenceError>;

    /// Название модели (для логов и метрик).
    fn model_name(&self) -> &str;

    /// Максимальный размер контекстного окна в токенах.
    fn max_context_tokens(&self) -> usize;

    /// Проверяет доступность бэкенда.
    async fn health_check(&self) -> bool;

    /// Создаёт бэкенд из конфигурации.
    async fn from_config(config: &BackendConfig) -> Result<Self, InferenceError>
    where
        Self: Sized;
}
