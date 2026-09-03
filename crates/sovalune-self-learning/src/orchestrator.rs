use crate::{LearningCycle, LearningCycleStatus};
use sovalune_storage_client::{LearningCycleRepository, CreateLearningCycle, CreateEvidence, CreateTestResult, LearningCycleEvidence, LearningCycleTestResult};
use sovalune_bus::NatsClient;
use sqlx::PgPool;
use uuid::Uuid;
use tracing::{info, warn, error};

#[derive(Clone)]
pub struct LearningCycleOrchestrator {
    repo: LearningCycleRepository,
    nats: Option<NatsClient>,
    max_retries: i32,
    #[allow(dead_code)]
    min_confidence: f32,
    max_failed_cycles: i32,
}

impl LearningCycleOrchestrator {
    pub fn new(pool: PgPool) -> Self {
        Self {
            repo: LearningCycleRepository::new(pool),
            nats: None,
            max_retries: 3,
            min_confidence: 0.7,
            max_failed_cycles: 5,
        }
    }
    
    pub fn with_nats(pool: PgPool, nats: NatsClient) -> Self {
        Self {
            repo: LearningCycleRepository::new(pool),
            nats: Some(nats),
            max_retries: 3,
            min_confidence: 0.7,
            max_failed_cycles: 5,
        }
    }
    
    pub async fn start_cycle(
        &self,
        project_id: Uuid,
        origin_task_id: Uuid,
    ) -> anyhow::Result<LearningCycle> {
        let recent_failures = self.repo.count_failed_recent(project_id, 10).await?;
        if recent_failures >= self.max_failed_cycles as i64 {
            warn!("Circuit breaker: too many failed cycles for project {}", project_id);
            return Err(anyhow::anyhow!("Circuit breaker: too many failed cycles"));
        }
        
        let cycle = self.repo.create(CreateLearningCycle {
            project_id,
            origin_task_id,
        }).await?;
        
        info!("Started learning cycle: {} for project {}", cycle.id, project_id);
        
        self.publish_event("learning.cycle.started", &cycle.id, &project_id, serde_json::json!({})).await;
        
        let learning_cycle = LearningCycle {
            id: cycle.id,
            project_id: cycle.project_id,
            status: LearningCycleStatus::Detected,
            origin_task_id: cycle.origin_task_id,
            failure_reason: cycle.failure_reason,
            retry_count: cycle.retry_count,
            confidence_score: cycle.confidence_score,
            created_at: cycle.created_at,
            updated_at: cycle.updated_at,
        };
        
        Ok(learning_cycle)
    }
    
    pub async fn advance_cycle(&self, cycle_id: Uuid) -> anyhow::Result<LearningCycle> {
        let cycle = self.repo.get(cycle_id).await?
            .ok_or_else(|| anyhow::anyhow!("Cycle not found: {}", cycle_id))?;
        
        let current_status: LearningCycleStatus = cycle.status.parse()?;
        
        if current_status.is_terminal() {
            return Err(anyhow::anyhow!("Cycle is already in terminal state: {}", cycle.status));
        }
        
        let next_status = current_status.next()
            .ok_or_else(|| anyhow::anyhow!("Cannot advance from {}", cycle.status))?;
        
        if cycle.retry_count >= self.max_retries {
            self.repo.update_status(cycle_id, "failed", Some("Max retries exceeded")).await?;
            self.publish_event("learning.cycle.finished", &cycle_id, &cycle.project_id, serde_json::json!({"reason": "max_retries"})).await;
            return Err(anyhow::anyhow!("Max retries exceeded"));
        }
        
        self.repo.update_status(cycle_id, &next_status.to_string(), None).await?;
        
        self.publish_stage_change(&cycle_id, &cycle.project_id, &current_status, &next_status, serde_json::json!({})).await;
        
        info!("Advanced cycle {} from {} to {}", cycle_id, current_status, next_status);
        
        let updated = self.repo.get(cycle_id).await?
            .ok_or_else(|| anyhow::anyhow!("Cycle not found after update"))?;
        
        Ok(LearningCycle {
            id: updated.id,
            project_id: updated.project_id,
            status: updated.status.parse()?,
            origin_task_id: updated.origin_task_id,
            failure_reason: updated.failure_reason,
            retry_count: updated.retry_count,
            confidence_score: updated.confidence_score,
            created_at: updated.created_at,
            updated_at: updated.updated_at,
        })
    }
    
    pub async fn fail_cycle(&self, cycle_id: Uuid, reason: &str) -> anyhow::Result<LearningCycle> {
        self.repo.update_status(cycle_id, "failed", Some(reason)).await?;
        
        let cycle = self.repo.get(cycle_id).await?
            .ok_or_else(|| anyhow::anyhow!("Cycle not found: {}", cycle_id))?;
        
        self.publish_event("learning.cycle.finished", &cycle_id, &cycle.project_id, serde_json::json!({"reason": reason})).await;
        
        warn!("Failed cycle {}: {}", cycle_id, reason);
        
        Ok(LearningCycle {
            id: cycle.id,
            project_id: cycle.project_id,
            status: LearningCycleStatus::Failed,
            origin_task_id: cycle.origin_task_id,
            failure_reason: cycle.failure_reason,
            retry_count: cycle.retry_count,
            confidence_score: cycle.confidence_score,
            created_at: cycle.created_at,
            updated_at: cycle.updated_at,
        })
    }
    
    pub async fn retry_cycle(&self, cycle_id: Uuid) -> anyhow::Result<LearningCycle> {
        let cycle = self.repo.get(cycle_id).await?
            .ok_or_else(|| anyhow::anyhow!("Cycle not found: {}", cycle_id))?;
        
        if cycle.retry_count >= self.max_retries {
            return Err(anyhow::anyhow!("Max retries exceeded"));
        }
        
        self.repo.increment_retry(cycle_id).await?;
        self.repo.update_status(cycle_id, "researching", None).await?;
        
        info!("Retrying cycle {} (attempt {})", cycle_id, cycle.retry_count + 1);
        
        self.advance_cycle(cycle_id).await
    }
    
    pub async fn get_cycle(&self, cycle_id: Uuid) -> anyhow::Result<LearningCycle> {
        let cycle = self.repo.get(cycle_id).await?
            .ok_or_else(|| anyhow::anyhow!("Cycle not found: {}", cycle_id))?;
        
        Ok(LearningCycle {
            id: cycle.id,
            project_id: cycle.project_id,
            status: cycle.status.parse()?,
            origin_task_id: cycle.origin_task_id,
            failure_reason: cycle.failure_reason,
            retry_count: cycle.retry_count,
            confidence_score: cycle.confidence_score,
            created_at: cycle.created_at,
            updated_at: cycle.updated_at,
        })
    }
    
    pub async fn list_cycles(
        &self,
        project_id: Option<Uuid>,
        status: Option<LearningCycleStatus>,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<LearningCycle>> {
        let status_str = status.map(|s| s.to_string());
        let cycles = self.repo.list(project_id, status_str.as_deref(), limit, offset).await?;
        
        let learning_cycles = cycles
            .into_iter()
            .filter_map(|c| {
                Some(LearningCycle {
                    id: c.id,
                    project_id: c.project_id,
                    status: c.status.parse().ok()?,
                    origin_task_id: c.origin_task_id,
                    failure_reason: c.failure_reason,
                    retry_count: c.retry_count,
                    confidence_score: c.confidence_score,
                    created_at: c.created_at,
                    updated_at: c.updated_at,
                })
            })
            .collect();
        
        Ok(learning_cycles)
    }
    
    pub async fn add_evidence(
        &self,
        cycle_id: Uuid,
        source_type: &str,
        source_url: Option<&str>,
        excerpt: &str,
        trust_tier: i32,
    ) -> anyhow::Result<()> {
        self.repo.add_evidence(CreateEvidence {
            cycle_id,
            source_type: source_type.to_string(),
            source_url: source_url.map(|s| s.to_string()),
            excerpt: excerpt.to_string(),
            trust_tier,
        }).await?;
        
        info!("Added evidence to cycle {}: {}", cycle_id, source_type);
        Ok(())
    }
    
    pub async fn get_evidence(&self, cycle_id: Uuid) -> anyhow::Result<Vec<LearningCycleEvidence>> {
        self.repo.get_evidence(cycle_id).await
    }
    
    pub async fn add_test_result(
        &self,
        cycle_id: Uuid,
        stage: &str,
        passed: bool,
        detail: serde_json::Value,
    ) -> anyhow::Result<()> {
        self.repo.add_test_result(CreateTestResult {
            cycle_id,
            stage: stage.to_string(),
            passed,
            detail,
        }).await?;
        
        info!("Added test result to cycle {}: passed={}", cycle_id, passed);
        Ok(())
    }
    
    pub async fn get_test_results(&self, cycle_id: Uuid) -> anyhow::Result<Vec<LearningCycleTestResult>> {
        self.repo.get_test_results(cycle_id).await
    }
    
    pub async fn update_confidence(&self, cycle_id: Uuid, confidence: f32) -> anyhow::Result<()> {
        self.repo.update_confidence(cycle_id, confidence).await
    }
    
    async fn publish_stage_change(
        &self,
        cycle_id: &Uuid,
        project_id: &Uuid,
        from: &LearningCycleStatus,
        to: &LearningCycleStatus,
        detail: serde_json::Value,
    ) {
        let payload = serde_json::json!({
            "cycle_id": cycle_id,
            "project_id": project_id,
            "from_status": from.to_string(),
            "to_status": to.to_string(),
            "detail": detail,
        });
        
        self.publish_event("learning.cycle.stage_changed", cycle_id, project_id, payload).await;
    }
    
    async fn publish_event(&self, subject: &str, _cycle_id: &Uuid, project_id: &Uuid, payload: serde_json::Value) {
        if let Some(nats) = &self.nats {
            let full_subject = format!("{}.{}", subject, project_id);
            if let Err(e) = nats.client().publish(full_subject, serde_json::to_vec(&payload).unwrap().into()).await {
                error!("Failed to publish NATS event: {}", e);
            }
        }
    }
}
