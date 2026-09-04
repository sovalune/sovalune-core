use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LearningCycle {
    pub id: Uuid,
    pub project_id: Uuid,
    pub status: String,
    pub origin_task_id: Uuid,
    pub failure_reason: Option<String>,
    pub retry_count: i32,
    pub confidence_score: Option<f32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LearningCycleEvidence {
    pub id: Uuid,
    pub cycle_id: Uuid,
    pub source_type: String,
    pub source_url: Option<String>,
    pub excerpt: String,
    pub trust_tier: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LearningCycleTestResult {
    pub id: Uuid,
    pub cycle_id: Uuid,
    pub stage: String,
    pub passed: bool,
    pub detail: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLearningCycle {
    pub project_id: Uuid,
    pub origin_task_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEvidence {
    pub cycle_id: Uuid,
    pub source_type: String,
    pub source_url: Option<String>,
    pub excerpt: String,
    pub trust_tier: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTestResult {
    pub cycle_id: Uuid,
    pub stage: String,
    pub passed: bool,
    pub detail: serde_json::Value,
}

#[derive(Clone)]
pub struct LearningCycleRepository {
    pool: PgPool,
}

impl LearningCycleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, cycle: CreateLearningCycle) -> anyhow::Result<LearningCycle> {
        let id = Uuid::new_v4();

        let row = sqlx::query_as::<_, LearningCycle>(
            r#"
            INSERT INTO learning_cycles (id, project_id, status, origin_task_id)
            VALUES ($1, $2, 'detected', $3)
            RETURNING id, project_id, status::text, origin_task_id, failure_reason, retry_count, confidence_score, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(cycle.project_id)
        .bind(cycle.origin_task_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get(&self, id: Uuid) -> anyhow::Result<Option<LearningCycle>> {
        let row = sqlx::query_as::<_, LearningCycle>(
            r#"
            SELECT id, project_id, status::text, origin_task_id, failure_reason, retry_count, confidence_score, created_at, updated_at
            FROM learning_cycles
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    pub async fn update_status(
        &self,
        id: Uuid,
        status: &str,
        failure_reason: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            UPDATE learning_cycles
            SET status = $1, failure_reason = $2, updated_at = NOW()
            WHERE id = $3
            "#,
        )
        .bind(status)
        .bind(failure_reason)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn increment_retry(&self, id: Uuid) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            UPDATE learning_cycles
            SET retry_count = retry_count + 1, updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_confidence(&self, id: Uuid, confidence_score: f32) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            UPDATE learning_cycles
            SET confidence_score = $1, updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(confidence_score)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn list(
        &self,
        project_id: Option<Uuid>,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<LearningCycle>> {
        let mut query = String::from(
            r#"
            SELECT id, project_id, status::text, origin_task_id, failure_reason, retry_count, confidence_score, created_at, updated_at
            FROM learning_cycles
            WHERE 1=1
            "#,
        );

        if let Some(project_id) = project_id {
            query.push_str(&format!(" AND project_id = '{}'", project_id));
        }
        if let Some(status) = status {
            query.push_str(&format!(" AND status = '{}'", status));
        }

        query.push_str(" ORDER BY created_at DESC");
        query.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

        let rows = sqlx::query_as::<_, LearningCycle>(&query)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows)
    }

    pub async fn add_evidence(
        &self,
        evidence: CreateEvidence,
    ) -> anyhow::Result<LearningCycleEvidence> {
        let id = Uuid::new_v4();

        let row = sqlx::query_as::<_, LearningCycleEvidence>(
            r#"
            INSERT INTO learning_cycle_evidence (id, cycle_id, source_type, source_url, excerpt, trust_tier)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, cycle_id, source_type, source_url, excerpt, trust_tier, created_at
            "#,
        )
        .bind(id)
        .bind(evidence.cycle_id)
        .bind(&evidence.source_type)
        .bind(&evidence.source_url)
        .bind(&evidence.excerpt)
        .bind(evidence.trust_tier)
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_evidence(&self, cycle_id: Uuid) -> anyhow::Result<Vec<LearningCycleEvidence>> {
        let rows = sqlx::query_as::<_, LearningCycleEvidence>(
            r#"
            SELECT id, cycle_id, source_type, source_url, excerpt, trust_tier, created_at
            FROM learning_cycle_evidence
            WHERE cycle_id = $1
            ORDER BY trust_tier DESC
            "#,
        )
        .bind(cycle_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn add_test_result(
        &self,
        result: CreateTestResult,
    ) -> anyhow::Result<LearningCycleTestResult> {
        let id = Uuid::new_v4();

        let row = sqlx::query_as::<_, LearningCycleTestResult>(
            r#"
            INSERT INTO learning_cycle_test_results (id, cycle_id, stage, passed, detail)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, cycle_id, stage, passed, detail, created_at
            "#,
        )
        .bind(id)
        .bind(result.cycle_id)
        .bind(&result.stage)
        .bind(result.passed)
        .bind(&result.detail)
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_test_results(
        &self,
        cycle_id: Uuid,
    ) -> anyhow::Result<Vec<LearningCycleTestResult>> {
        let rows = sqlx::query_as::<_, LearningCycleTestResult>(
            r#"
            SELECT id, cycle_id, stage, passed, detail, created_at
            FROM learning_cycle_test_results
            WHERE cycle_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(cycle_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn count_failed_recent(&self, project_id: Uuid, limit: i64) -> anyhow::Result<i64> {
        let row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM (
                SELECT id
                FROM learning_cycles
                WHERE project_id = $1 AND status = 'failed'
                ORDER BY created_at DESC
                LIMIT $2
            ) sub
            "#,
        )
        .bind(project_id)
        .bind(limit)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0)
    }
}
