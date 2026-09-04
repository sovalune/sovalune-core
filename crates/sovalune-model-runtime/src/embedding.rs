//! Модуль генерации эмбеддингов для семантического поиска.
//!
//! Поддерживает OpenAI Embeddings API и локальные модели (Ollama, sentence-transformers).
//!
//! # Протокол
//!
//! Запрос: `POST /embeddings` с body `{"model": "...", "input": ["..."]}`
//! Ответ: `{"data": [{"embedding": [0.1, ...], "index": 0}]}`

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::debug;

/// Ошибка генерации эмбеддинга.
#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error {status}: {body}")]
    ApiError { status: u16, body: String },

    #[error("Backend unavailable: {0}")]
    BackendUnavailable(String),

    #[error("Timeout after {0}ms")]
    Timeout(u64),

    #[error("Dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },

    #[error("Empty input")]
    EmptyInput,

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Трейт для генерации эмбеддингов.
#[async_trait]
pub trait EmbeddingBackend: Send + Sync {
    /// Генерирует эмбеддинг для текста.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError>;

    /// Генерирует эмбеддинги для пакета текстов.
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError>;

    /// Возвращает размерность эмбеддингов.
    fn dimensions(&self) -> usize;

    /// Возвращает название модели.
    fn model_name(&self) -> &str;

    /// Проверяет здоровье бэкенда.
    async fn health_check(&self) -> bool;
}

/// OpenAI Embeddings API бэкенд.
pub struct OpenAIEmbeddingBackend {
    client: Client,
    api_url: String,
    api_key: String,
    model: String,
    dimensions: usize,
}

impl OpenAIEmbeddingBackend {
    pub fn new(api_url: &str, api_key: &str, model: &str, dimensions: usize) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            api_url: api_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            dimensions,
        }
    }
}

#[async_trait]
impl EmbeddingBackend for OpenAIEmbeddingBackend {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let results = self.embed_batch(&[text]).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| EmbeddingError::EmptyInput)
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() {
            return Err(EmbeddingError::EmptyInput);
        }

        let url = format!("{}/embeddings", self.api_url);

        #[derive(Serialize)]
        struct EmbeddingRequest {
            model: String,
            input: Vec<String>,
        }

        #[derive(Deserialize)]
        struct EmbeddingResponse {
            data: Vec<EmbeddingData>,
        }

        #[derive(Deserialize)]
        struct EmbeddingData {
            embedding: Vec<f32>,
            index: usize,
        }

        let request = EmbeddingRequest {
            model: self.model.clone(),
            input: texts.iter().map(|s| s.to_string()).collect(),
        };

        debug!(
            "Generating embeddings: model={}, count={}",
            self.model,
            texts.len()
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::ApiError { status, body });
        }

        let embedding_response: EmbeddingResponse = response.json().await?;

        let mut results: Vec<(usize, Vec<f32>)> = embedding_response
            .data
            .into_iter()
            .map(|d| (d.index, d.embedding))
            .collect();

        results.sort_by_key(|(index, _)| *index);

        Ok(results
            .into_iter()
            .map(|(_, embedding)| embedding)
            .collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    async fn health_check(&self) -> bool {
        let result = self.embed("test").await;
        result.is_ok()
    }
}

/// Локальный бэкенд для эмбеддингов (Ollama, sentence-transformers).
pub struct LocalEmbeddingBackend {
    client: Client,
    server_url: String,
    model: String,
    dimensions: usize,
}

impl LocalEmbeddingBackend {
    pub fn new(server_url: &str, model: &str, dimensions: usize) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            server_url: server_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            dimensions,
        }
    }

    /// Автоматически определяет модель для эмбеддингов.
    pub async fn detect_model(server_url: &str) -> Result<String, EmbeddingError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| EmbeddingError::Internal(e.to_string()))?;

        // Пробуем Ollama API
        let ollama_url = format!("{}/api/tags", server_url);
        if let Ok(resp) = client.get(&ollama_url).send().await {
            if resp.status().is_success() {
                if let Ok(data) = resp.json::<serde_json::Value>().await {
                    if let Some(models) = data["models"].as_array() {
                        // Ищем модель для эмбеддингов
                        for model in models {
                            if let Some(name) = model["name"].as_str() {
                                if name.contains("embed")
                                    || name.contains("bge")
                                    || name.contains("nomic")
                                {
                                    info!("Detected embedding model: {}", name);
                                    return Ok(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok("nomic-embed-text".to_string())
    }
}

#[async_trait]
impl EmbeddingBackend for LocalEmbeddingBackend {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let results = self.embed_batch(&[text]).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| EmbeddingError::EmptyInput)
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() {
            return Err(EmbeddingError::EmptyInput);
        }

        let url = format!("{}/api/embed", self.server_url);

        #[derive(Serialize)]
        struct EmbedRequest {
            model: String,
            input: Vec<String>,
        }

        #[derive(Deserialize)]
        struct EmbedResponse {
            embeddings: Vec<Vec<f32>>,
        }

        let request = EmbedRequest {
            model: self.model.clone(),
            input: texts.iter().map(|s| s.to_string()).collect(),
        };

        debug!(
            "Local embedding: model={}, count={}",
            self.model,
            texts.len()
        );

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::ApiError { status, body });
        }

        let embedding_response: EmbedResponse = response.json().await?;
        Ok(embedding_response.embeddings)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    async fn health_check(&self) -> bool {
        let result = self.embed("test").await;
        result.is_ok()
    }
}

use tracing::info;

/// Фабрика эмбеддинг-бэкендов.
pub struct EmbeddingFactory;

impl EmbeddingFactory {
    /// Создаёт эмбеддинг-бэкенд из конфигурации.
    pub async fn create(
        backend_type: &str,
        api_url: &str,
        api_key: Option<&str>,
        model: &str,
        dimensions: usize,
    ) -> Result<Box<dyn EmbeddingBackend>, EmbeddingError> {
        match backend_type {
            "openai" => {
                let key = api_key.ok_or_else(|| {
                    EmbeddingError::Internal("API key required for OpenAI embedding".into())
                })?;
                Ok(Box::new(OpenAIEmbeddingBackend::new(
                    api_url, key, model, dimensions,
                )))
            }
            "local" => {
                let actual_model = if model.is_empty() {
                    LocalEmbeddingBackend::detect_model(api_url).await?
                } else {
                    model.to_string()
                };
                Ok(Box::new(LocalEmbeddingBackend::new(
                    api_url,
                    &actual_model,
                    dimensions,
                )))
            }
            other => Err(EmbeddingError::Internal(format!(
                "Unknown embedding backend: {}",
                other
            ))),
        }
    }
}
