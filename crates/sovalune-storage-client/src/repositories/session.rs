use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Session {
    pub id: Uuid,
    pub project_id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Message {
    pub id: Uuid,
    pub session_id: Uuid,
    pub role: String,
    pub content: String,
    pub tool_call: Option<serde_json::Value>,
    pub request_id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSession {
    pub project_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMessage {
    pub session_id: Uuid,
    pub role: String,
    pub content: String,
    pub tool_call: Option<serde_json::Value>,
    pub request_id: Uuid,
}

#[derive(Clone)]
pub struct SessionRepository {
    pool: PgPool,
}

impl SessionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_session(&self, project_id: Uuid) -> anyhow::Result<Session> {
        let id = Uuid::new_v4();

        let row = sqlx::query_as::<_, Session>(
            r#"
            INSERT INTO sessions (id, project_id)
            VALUES ($1, $2)
            RETURNING id, project_id, created_at
            "#,
        )
        .bind(id)
        .bind(project_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_session(&self, id: Uuid) -> anyhow::Result<Option<Session>> {
        let row = sqlx::query_as::<_, Session>(
            r#"
            SELECT id, project_id, created_at
            FROM sessions
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    pub async fn list_sessions(
        &self,
        project_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<Session>> {
        let rows = sqlx::query_as::<_, Session>(
            r#"
            SELECT id, project_id, created_at
            FROM sessions
            WHERE project_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(project_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn create_message(&self, message: CreateMessage) -> anyhow::Result<Message> {
        let id = Uuid::new_v4();

        let row = sqlx::query_as::<_, Message>(
            r#"
            INSERT INTO messages (id, session_id, role, content, tool_call, request_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, session_id, role::text, content, tool_call, request_id, created_at
            "#,
        )
        .bind(id)
        .bind(message.session_id)
        .bind(&message.role)
        .bind(&message.content)
        .bind(&message.tool_call)
        .bind(message.request_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_messages(&self, session_id: Uuid, limit: i64) -> anyhow::Result<Vec<Message>> {
        let rows = sqlx::query_as::<_, Message>(
            r#"
            SELECT id, session_id, role::text, content, tool_call, request_id, created_at
            FROM messages
            WHERE session_id = $1
            ORDER BY created_at ASC
            LIMIT $2
            "#,
        )
        .bind(session_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_recent_messages(
        &self,
        session_id: Uuid,
        limit: i64,
    ) -> anyhow::Result<Vec<Message>> {
        let rows = sqlx::query_as::<_, Message>(
            r#"
            SELECT id, session_id, role::text, content, tool_call, request_id, created_at
            FROM (
                SELECT id, session_id, role, content, tool_call, request_id, created_at
                FROM messages
                WHERE session_id = $1
                ORDER BY created_at DESC
                LIMIT $2
            ) sub
            ORDER BY created_at ASC
            "#,
        )
        .bind(session_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
}
