//! Типы данных для инференса моделей.
//!
//! Содержит структуры запросов, ответов и конфигурации,
//! используемые во всём pipeline инференса.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Конфигурация генерации — параметры, управляемые клиентом.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    /// Максимальное количество токенов в ответе.
    pub max_tokens: u32,
    /// Температура генерации (0.0 — детерминированно, 2.0 — максимально случайно).
    pub temperature: f32,
    /// Top-p sampling (nucleus sampling).
    #[serde(default = "default_top_p")]
    pub top_p: f32,
    /// Stream-режим — генерировать токены по мере поступления.
    #[serde(default = "default_true")]
    pub stream: bool,
}

fn default_top_p() -> f32 {
    0.9
}

fn default_true() -> bool {
    true
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            max_tokens: 2048,
            temperature: 0.7,
            top_p: 0.9,
            stream: true,
        }
    }
}

/// Секция контекста — блок информации для промпта.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSection {
    /// Роль секции (system, memory, history, user).
    pub role: String,
    /// Содержимое секции.
    pub content: String,
    /// Приоритет секции (чем выше — тем ближе к началу промпта).
    pub priority: u32,
    /// Токен-оценка размера секции.
    pub token_estimate: usize,
}

/// Запрос на инференс — полный контекст для генерации.
#[derive(Debug, Clone)]
pub struct InferenceRequest {
    /// Уникальный ID запроса.
    pub id: Uuid,
    /// ID сессии.
    pub session_id: String,
    /// ID проекта.
    pub project_id: String,
    /// Собранный контекст (секции промпта).
    pub context: Vec<ContextSection>,
    /// Пользовательский ввод.
    pub user_input: String,
    /// Конфигурация генерации.
    pub config: GenerationConfig,
}

/// Событие токена — один фрагмент ответа модели.
#[derive(Debug, Clone)]
pub struct TokenEvent {
    /// ID запроса.
    pub request_id: Uuid,
    /// Один токен (слово/символ).
    pub delta: String,
    /// Завершена ли генерация.
    pub finished: bool,
    /// ID сообщения (заполняется при завершении).
    pub message_id: Option<String>,
}

/// Результат инференса — полный ответ модели.
#[derive(Debug, Clone)]
pub struct InferenceResult {
    /// ID запроса.
    pub request_id: Uuid,
    /// Полный текст ответа.
    pub content: String,
    /// Использовано токенов.
    pub tokens_used: u32,
    /// Модель, использованная для генерации.
    pub model: String,
    /// Время генерации (мс).
    pub duration_ms: u64,
}

/// Ошибка инференса.
#[derive(Debug, thiserror::Error)]
pub enum InferenceError {
    #[error("Backend unavailable: {0}")]
    BackendUnavailable(String),

    #[error("Rate limited, retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("Context too long: {tokens} tokens exceeds max {max_tokens}")]
    ContextTooLong { tokens: usize, max_tokens: usize },

    #[error("Model error: {0}")]
    ModelError(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Timeout after {0}ms")]
    Timeout(u64),

    #[error("Internal error: {0}")]
    Internal(String),
}
