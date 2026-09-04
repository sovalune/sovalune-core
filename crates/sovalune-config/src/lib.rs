use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub storage_url: String,
    pub nats_url: String,
    pub server_host: String,
    pub server_port: u16,

    /// Тип бэкенда моделей: "openai", "local".
    pub model_backend: String,
    /// URL API моделей.
    pub model_api_url: String,
    /// API ключ моделей (опционально для локальных).
    pub model_api_key: Option<String>,
    /// Имя модели.
    pub model_name: String,
    /// Таймаут запроса к модели (сек).
    pub model_timeout_secs: u64,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            storage_url: std::env::var("SOVALUNE_STORAGE_URL").unwrap_or_else(|_| {
                "postgres://sovalune:sovalune_dev_password@localhost:5432/sovalune".to_string()
            }),
            nats_url: std::env::var("SOVALUNE_NATS_URL")
                .unwrap_or_else(|_| "nats://localhost:4222".to_string()),
            server_host: std::env::var("SOVALUNE_SERVER_HOST")
                .unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_port: std::env::var("SOVALUNE_SERVER_PORT")
                .unwrap_or_else(|_| "8090".to_string())
                .parse()?,

            model_backend: std::env::var("SOVALUNE_MODEL_BACKEND")
                .unwrap_or_else(|_| "openai".to_string()),
            model_api_url: std::env::var("SOVALUNE_MODEL_API_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            model_api_key: std::env::var("SOVALUNE_MODEL_API_KEY").ok(),
            model_name: std::env::var("SOVALUNE_MODEL_NAME")
                .unwrap_or_else(|_| "gpt-4".to_string()),
            model_timeout_secs: std::env::var("SOVALUNE_MODEL_TIMEOUT_SECS")
                .unwrap_or_else(|_| "120".to_string())
                .parse()
                .unwrap_or(120),
        })
    }
}
