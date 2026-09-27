use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskBoardResponse {
    pub active_tasks: Vec<TaskSummary>,
    pub completed_tasks: Vec<TaskSummary>,
    pub failed_tasks: Vec<TaskSummary>,
    pub paused_tasks: Vec<TaskSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub task_id: String,
    pub kind: String,
    pub owner_agent: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub progress: f32, // 0.0 to 1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTimeline {
    pub task_id: String,
    pub checkpoints: Vec<TaskCheckpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCheckpoint {
    pub step_name: String,
    pub status: String, // "pending", "running", "success", "failed"
    pub timestamp: String,
    pub logs: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHealthSnapshot {
    pub active_sessions: usize,
    pub circuit_breakers: Vec<CircuitBreakerStatus>,
    pub provider_health: String,
    pub compaction_outcomes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardCorrectionControls {
    pub allow_inspection: bool,
    pub allow_correction: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerStatus {
    pub provider_name: String,
    pub state: String, // "closed", "open", "half_open"
    pub error_rate: f32,
    pub next_retry: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub version: u32,
    pub created_at: String,
    pub files: Vec<String>,
    pub size_bytes: u64,
}
