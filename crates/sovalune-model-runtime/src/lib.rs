//! # Sovalune Model Runtime
//!
//! Центральный модуль для инференса моделей AI в платформе Sovalune.
//! Предоставляет абстракцию над различными бэкендами моделей (OpenAI, локальные)
//! и управляет потоковым генерацией ответов.
//!
//! ## Архитектура
//!
//! ```text
//! InferenceRequest → ContextBuilder → ModelBackend → TokenStream → InferenceResponse
//!                    ↓                                       ↓
//!               TokenCounter                             ToolExecutor
//!                    ↓                                       ↓
//!               CacheLayer                              ToolRegistry
//! ```
//!
//! ## Модули
//!
//! - **backend** — абстракция бэкендов моделей (OpenAI, Local)
//! - **context** — сборщик контекста для промпта
//! - **engine** — оркестратор инференса
//! - **types** — общие типы данных
//! - **embedding** — генерация эмбеддингов для семантического поиска
//! - **tools** — вызов инструментов (function calling)
//! - **retry** — повторные попытки с экспоненциальной задержкой
//! - **cache** — кеширование запросов и ответов
//! - **tokenizer** — подсчёт токенов

pub mod backend;
pub mod cache;
pub mod context;
pub mod embedding;
pub mod engine;
pub mod retry;
pub mod tokenizer;
pub mod tools;
pub mod types;

pub use backend::{BackendConfig, LocalBackend, ModelBackend, OpenAIBackend};
pub use cache::{Cache, CacheFactory, EmbeddingCache, ResponseCache, TokenCountCache};
pub use context::ContextBuilder;
pub use embedding::{
    EmbeddingBackend, EmbeddingError, EmbeddingFactory, LocalEmbeddingBackend,
    OpenAIEmbeddingBackend,
};
pub use engine::InferenceEngine;
pub use retry::{RetryConfig, RetryState};
pub use tokenizer::{ContextLimits, ContextMessage, TokenCounter};
pub use tools::{
    ToolCall, ToolCallManager, ToolCallParser, ToolDefinition, ToolError, ToolExecutor,
    ToolRegistry, ToolResult,
};
pub use types::*;
