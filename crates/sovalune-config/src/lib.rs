use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub storage_url: String,
    pub nats_url: String,
    pub server_host: String,
    pub server_port: u16,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            storage_url: std::env::var("SOVALUNE_STORAGE_URL")
                .unwrap_or_else(|_| "postgres://sovalune:sovalune_dev_password@localhost:5432/sovalune".to_string()),
            nats_url: std::env::var("SOVALUNE_NATS_URL")
                .unwrap_or_else(|_| "nats://localhost:4222".to_string()),
            server_host: std::env::var("SOVALUNE_SERVER_HOST")
                .unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_port: std::env::var("SOVALUNE_SERVER_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()?,
        })
    }
}
