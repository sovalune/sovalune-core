use sqlx::PgPool;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub settings: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProject {
    pub name: String,
    pub settings: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProject {
    pub name: Option<String>,
    pub settings: Option<serde_json::Value>,
}

#[derive(Clone)]
pub struct ProjectRepository {
    pool: PgPool,
}

impl ProjectRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create(&self, project: CreateProject) -> anyhow::Result<Project> {
        let id = Uuid::new_v4();
        
        let row = sqlx::query_as::<_, Project>(
            r#"
            INSERT INTO projects (id, name, settings)
            VALUES ($1, $2, $3)
            RETURNING id, name, settings, created_at
            "#,
        )
        .bind(id)
        .bind(&project.name)
        .bind(project.settings.unwrap_or(serde_json::json!({})))
        .fetch_one(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    pub async fn get(&self, id: Uuid) -> anyhow::Result<Option<Project>> {
        let row = sqlx::query_as::<_, Project>(
            r#"
            SELECT id, name, settings, created_at
            FROM projects
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    pub async fn update(&self, id: Uuid, update: UpdateProject) -> anyhow::Result<Option<Project>> {
        if let Some(name) = update.name {
            sqlx::query("UPDATE projects SET name = $1 WHERE id = $2")
                .bind(name)
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        if let Some(settings) = update.settings {
            sqlx::query("UPDATE projects SET settings = $1 WHERE id = $2")
                .bind(settings)
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        
        self.get(id).await
    }
    
    pub async fn list(&self, limit: i64, offset: i64) -> anyhow::Result<Vec<Project>> {
        let rows = sqlx::query_as::<_, Project>(
            r#"
            SELECT id, name, settings, created_at
            FROM projects
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
    
    pub async fn delete(&self, id: Uuid) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM projects WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
}
