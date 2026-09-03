pub mod orchestrator;
pub mod stages;

pub use orchestrator::LearningCycleOrchestrator;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LearningCycleStatus {
    Detected,
    Researching,
    Verifying,
    Practicing,
    Testing,
    Applying,
    Completed,
    Failed,
}

impl std::fmt::Display for LearningCycleStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LearningCycleStatus::Detected => write!(f, "detected"),
            LearningCycleStatus::Researching => write!(f, "researching"),
            LearningCycleStatus::Verifying => write!(f, "verifying"),
            LearningCycleStatus::Practicing => write!(f, "practicing"),
            LearningCycleStatus::Testing => write!(f, "testing"),
            LearningCycleStatus::Applying => write!(f, "applying"),
            LearningCycleStatus::Completed => write!(f, "completed"),
            LearningCycleStatus::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for LearningCycleStatus {
    type Err = anyhow::Error;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "detected" => Ok(LearningCycleStatus::Detected),
            "researching" => Ok(LearningCycleStatus::Researching),
            "verifying" => Ok(LearningCycleStatus::Verifying),
            "practicing" => Ok(LearningCycleStatus::Practicing),
            "testing" => Ok(LearningCycleStatus::Testing),
            "applying" => Ok(LearningCycleStatus::Applying),
            "completed" => Ok(LearningCycleStatus::Completed),
            "failed" => Ok(LearningCycleStatus::Failed),
            _ => Err(anyhow::anyhow!("Invalid status: {}", s)),
        }
    }
}

impl LearningCycleStatus {
    pub fn next(&self) -> Option<Self> {
        match self {
            LearningCycleStatus::Detected => Some(LearningCycleStatus::Researching),
            LearningCycleStatus::Researching => Some(LearningCycleStatus::Verifying),
            LearningCycleStatus::Verifying => Some(LearningCycleStatus::Practicing),
            LearningCycleStatus::Practicing => Some(LearningCycleStatus::Testing),
            LearningCycleStatus::Testing => Some(LearningCycleStatus::Applying),
            LearningCycleStatus::Applying => Some(LearningCycleStatus::Completed),
            _ => None,
        }
    }
    
    pub fn is_terminal(&self) -> bool {
        matches!(self, LearningCycleStatus::Completed | LearningCycleStatus::Failed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningCycle {
    pub id: Uuid,
    pub project_id: Uuid,
    pub status: LearningCycleStatus,
    pub origin_task_id: Uuid,
    pub failure_reason: Option<String>,
    pub retry_count: i32,
    pub confidence_score: Option<f32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningCycleEvidence {
    pub id: Uuid,
    pub cycle_id: Uuid,
    pub source_type: String,
    pub source_url: Option<String>,
    pub excerpt: String,
    pub trust_tier: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningCycleTestResult {
    pub id: Uuid,
    pub cycle_id: Uuid,
    pub stage: String,
    pub passed: bool,
    pub detail: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageTransition {
    pub cycle_id: Uuid,
    pub from_status: LearningCycleStatus,
    pub to_status: LearningCycleStatus,
    pub detail: serde_json::Value,
}
