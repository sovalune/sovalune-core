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
pub mod context;
pub mod engine;
pub mod types;
pub mod embedding;
pub mod tools;
pub mod retry;
pub mod cache;
pub mod tokenizer;

pub use backend::{ModelBackend, OpenAIBackend, LocalBackend, BackendConfig};
pub use context::ContextBuilder;
pub use engine::InferenceEngine;
pub use types::*;
pub use embedding::{EmbeddingBackend, OpenAIEmbeddingBackend, LocalEmbeddingBackend, EmbeddingFactory, EmbeddingError};
pub use tools::{ToolExecutor, ToolRegistry, ToolCallManager, ToolCall, ToolResult, ToolDefinition, ToolError, ToolCallParser};
pub use retry::{RetryConfig, RetryState};
pub use cache::{Cache, CacheFactory, ResponseCache, EmbeddingCache, TokenCountCache};
pub use tokenizer::{TokenCounter, ContextMessage, ContextLimits};
