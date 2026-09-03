use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use tracing::info;

#[derive(Clone)]
pub struct StorageClient {
    pool: PgPool,
}

impl StorageClient {
    pub async fn new(database_url: &str) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(3))
            .connect(database_url)
            .await?;
        
        info!("Connected to PostgreSQL");
        
        Ok(Self { pool })
    }
    
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
    
    pub async fn run_migrations(&self) -> anyhow::Result<()> {
        // Run individual migrations from storage-schema
        let migrations = [
            include_str!("../../sovalune-storage-schema/migrations/001_extensions.sql"),
            include_str!("../../sovalune-storage-schema/migrations/002_projects.sql"),
            include_str!("../../sovalune-storage-schema/migrations/003_sessions_messages.sql"),
            include_str!("../../sovalune-storage-schema/migrations/004_memory_entries.sql"),
            include_str!("../../sovalune-storage-schema/migrations/005_learning_cycles.sql"),
            include_str!("../../sovalune-storage-schema/migrations/006_training_artifacts.sql"),
            include_str!("../../sovalune-storage-schema/migrations/007_rls_policies.sql"),
        ];
        
        for (i, migration) in migrations.iter().enumerate() {
            sqlx::query(migration).execute(&self.pool).await?;
            info!("Applied migration {}", i + 1);
        }
        
        info!("All migrations completed");
        Ok(())
    }
    
    pub async fn health_check(&self) -> bool {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .is_ok()
    }
}
