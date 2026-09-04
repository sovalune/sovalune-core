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
//! ```
//!
//! - **ContextBuilder** — собирает контекст из памяти, истории и системного промпта
//! - **ModelBackend** — абстракция над провайдером моделей (trait object)
//! - **TokenStream** — поток токенов от модели до клиента
//! - **InferenceEngine** — оркестратор, связывает всё воедино

pub mod backend;
pub mod context;
pub mod engine;
pub mod types;

pub use backend::{ModelBackend, OpenAIBackend, LocalBackend, BackendConfig};
pub use context::ContextBuilder;
pub use engine::InferenceEngine;
pub use types::*;
