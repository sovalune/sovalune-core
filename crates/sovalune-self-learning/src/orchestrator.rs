use crate::{LearningCycle, LearningCycleStatus};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct LearningCycleOrchestrator {
    pool: PgPool,
    max_retries: i32,
    min_confidence: f32,
}

impl LearningCycleOrchestrator {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            max_retries: 3,
            min_confidence: 0.7,
        }
    }

    pub async fn start_cycle(
        &self,
        project_id: Uuid,
        origin_task_id: Uuid,
    ) -> anyhow::Result<LearningCycle> {
        let id = Uuid::new_v4();
        
        sqlx::query(
            r#"
            INSERT INTO learning_cycles (id, project_id, status, origin_task_id, retry_count)
            VALUES ($1, $2, 'detected', $3, 0)
            "#,
        )
        .bind(id)
        .bind(project_id)
        .bind(origin_task_id)
        .execute(&self.pool)
        .await?;

        self.get_cycle(id).await
    }

    pub async fn advance_cycle(&self, cycle_id: Uuid) -> anyhow::Result<LearningCycle> {
        let cycle = self.get_cycle(cycle_id).await?;
        
        let next_status = cycle.status.next()
            .ok_or_else(|| anyhow::anyhow!("Cannot advance from {}", cycle.status))?;

        if cycle.retry_count >= self.max_retries {
            return Err(anyhow::anyhow!("Max retries exceeded"));
        }

        sqlx::query(
            r#"
            UPDATE learning_cycles
            SET status = $1, updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(next_status.to_string())
        .bind(cycle_id)
        .execute(&self.pool)
        .await?;

        self.get_cycle(cycle_id).await
    }

    pub async fn fail_cycle(&self, cycle_id: Uuid, reason: &str) -> anyhow::Result<LearningCycle> {
        sqlx::query(
            r#"
            UPDATE learning_cycles
            SET status = 'failed', failure_reason = $1, updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(reason)
        .bind(cycle_id)
        .execute(&self.pool)
        .await?;

        self.get_cycle(cycle_id).await
    }

    pub async fn retry_cycle(&self, cycle_id: Uuid) -> anyhow::Result<LearningCycle> {
        let cycle = self.get_cycle(cycle_id).await?;
        
        if cycle.retry_count >= self.max_retries {
            return Err(anyhow::anyhow!("Max retries exceeded"));
        }

        sqlx::query(
            r#"
            UPDATE learning_cycles
            SET status = 'researching', retry_count = retry_count + 1, updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(cycle_id)
        .execute(&self.pool)
        .await?;

        self.get_cycle(cycle_id).await
    }

    pub async fn get_cycle(&self, cycle_id: Uuid) -> anyhow::Result<LearningCycle> {
        let row = sqlx::query_as::<_, (Uuid, Uuid, String, Uuid, Option<String>, i32, Option<f32>, DateTime<Utc>, DateTime<Utc>)>(
            r#"
            SELECT id, project_id, status, origin_task_id, failure_reason, retry_count, confidence_score, created_at, updated_at
            FROM learning_cycles
            WHERE id = $1
            "#,
        )
        .bind(cycle_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Cycle not found: {}", cycle_id))?;

        Ok(LearningCycle {
            id: row.0,
            project_id: row.1,
            status: match row.2.as_str() {
                "detected" => LearningCycleStatus::Detected,
                "researching" => LearningCycleStatus::Researching,
                "verifying" => LearningCycleStatus::Verifying,
                "practicing" => LearningCycleStatus::Practicing,
                "testing" => LearningCycleStatus::Testing,
                "applying" => LearningCycleStatus::Applying,
                "completed" => LearningCycleStatus::Completed,
                "failed" => LearningCycleStatus::Failed,
                _ => LearningCycleStatus::Failed,
            },
            origin_task_id: row.3,
            failure_reason: row.4,
            retry_count: row.5,
            confidence_score: row.6,
            created_at: row.7,
            updated_at: row.8,
        })
    }

    pub async fn list_cycles(
        &self,
        project_id: Uuid,
        status: Option<LearningCycleStatus>,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<LearningCycle>> {
        let mut query = String::from(
            r#"
            SELECT id, project_id, status, origin_task_id, failure_reason, retry_count, confidence_score, created_at, updated_at
            FROM learning_cycles
            WHERE project_id = $1
            "#,
        );

        if let Some(status) = status {
            query.push_str(&format!(" AND status = '{}'", status));
        }

        query.push_str(" ORDER BY created_at DESC");
        query.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

        let rows = sqlx::query_as::<_, (Uuid, Uuid, String, Uuid, Option<String>, i32, Option<f32>, DateTime<Utc>, DateTime<Utc>)>(&query)
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?;

        let cycles = rows
            .into_iter()
            .map(|row| LearningCycle {
                id: row.0,
                project_id: row.1,
                status: match row.2.as_str() {
                    "detected" => LearningCycleStatus::Detected,
                    "researching" => LearningCycleStatus::Researching,
                    "verifying" => LearningCycleStatus::Verifying,
                    "practicing" => LearningCycleStatus::Practicing,
                    "testing" => LearningCycleStatus::Testing,
                    "applying" => LearningCycleStatus::Applying,
                    "completed" => LearningCycleStatus::Completed,
                    "failed" => LearningCycleStatus::Failed,
                    _ => LearningCycleStatus::Failed,
                },
                origin_task_id: row.3,
                failure_reason: row.4,
                retry_count: row.5,
                confidence_score: row.6,
                created_at: row.7,
                updated_at: row.8,
            })
            .collect();

        Ok(cycles)
    }
}

use chrono::Utc;
