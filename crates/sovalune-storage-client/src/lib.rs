use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use tracing::info;

mod repositories;

pub use repositories::*;

#[derive(Clone)]
pub struct StorageClient {
    pool: PgPool,
}

impl StorageClient {
    pub async fn new(database_url: &str) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(20)
            .acquire_timeout(Duration::from_secs(5))
            .connect(database_url)
            .await?;
        
        info!("Connected to PostgreSQL");
        
        Ok(Self { pool })
    }
    
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
    
    pub async fn run_migrations(&self) -> anyhow::Result<()> {
        sqlx::migrate!("../sovalune-storage-schema/migrations")
            .run(&self.pool)
            .await?;
        
        info!("Migrations completed");
        Ok(())
    }
    
    pub async fn health_check(&self) -> bool {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .is_ok()
    }
}
