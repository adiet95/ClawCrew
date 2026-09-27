//! TaskRunner specific state for multi-step autonomous workflows.

use serde::{Deserialize, Serialize};

use super::task_registry::{TaskRecord, TaskRegistry};
use super::task_store_sqlite::SqliteTaskStore;

pub const TASK_RUNNER_SCHEMA_VERSION: u16 = 1;
const MAX_ID_BYTES: usize = 128;
const MAX_TEXT_BYTES: usize = 16 * 1024;
const MAX_ARTIFACTS: usize = 128;
const MAX_STEPS: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunnerSpec {
    /// Version of this persisted workflow contract.
    #[serde(default = "current_schema_version")]
    pub schema_version: u16,
    /// Foreign key to the canonical [`TaskRecord`].
    pub task_id: String,
    /// Ordered steps defining the workflow.
    pub steps: Vec<TaskStep>,
    /// Index of the step currently being processed.
    #[serde(default)]
    pub current_step_index: usize,
    /// Last checkpoint that passed its verification boundary.
    #[serde(default)]
    pub checkpoint: Option<Checkpoint>,
    /// Bounded artifacts produced by the workflow.
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    /// Approval evidence required by one or more workflow steps.
    #[serde(default)]
    pub approval_gates: Vec<ApprovalGate>,
    /// Correlation ID stamped on every ledger/evidence entry this workflow
    /// emits, so validation and approval evidence can be traced back to the
    /// originating request/turn (P1.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
}

fn current_schema_version() -> u16 {
    TASK_RUNNER_SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub step_id: String,
    pub state_digest: String,
    #[serde(default)]
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub digest: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalGate {
    pub id: String,
    pub required_approvals: usize,
    #[serde(default)]
    pub granted_approvals: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStep {
    /// Stable identifier for this step.
    pub id: String,
    /// Human-readable step name.
    pub name: String,
    /// Detailed instruction or objective for this step.
    pub description: String,
    /// Acceptance criteria evaluated before considering the step complete.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation_gates: Vec<ValidationGate>,
    /// Step-level retry boundaries.
    #[serde(default)]
    pub retry_policy: RetryPolicy,
    /// Current step lifecycle state.
    #[serde(default)]
    pub status: StepStatus,
    /// Bounded output from the latest execution attempt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Bounded validation result for the latest attempt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationGate {
    /// Run a testing command (e.g., `cargo test`).
    Test { command: String },
    /// Run a linter command (e.g., `cargo clippy`).
    Lint { command: String },
    /// Run a build command (e.g., `cargo build`).
    Build { command: String },
    /// Require operator (user) approval.
    Review { required_approvals: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum attempts allowed.
    pub max_attempts: u32,
    /// Number of attempts executed so far.
    #[serde(default)]
    pub attempts: u32,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            attempts: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Executing,
    Validating,
    Paused(String),
    Failed(String),
    Completed,
}

impl StepStatus {
    pub fn can_transition_to(&self, next: &Self) -> bool {
        matches!(
            (self, next),
            (Self::Pending, Self::Executing)
                | (Self::Executing, Self::Validating)
                | (Self::Executing, Self::Paused(_))
                | (Self::Executing, Self::Failed(_))
                | (Self::Validating, Self::Completed)
                | (Self::Validating, Self::Failed(_))
                | (Self::Validating, Self::Paused(_))
                | (Self::Paused(_), Self::Executing)
                | (Self::Failed(_), Self::Executing)
        )
    }
}

impl Default for StepStatus {
    fn default() -> Self {
        Self::Pending
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    /// Task ID this ledger belongs to.
    pub task_id: String,
    /// Step ID this action relates to.
    pub step_id: String,
    /// Timestamp of the ledger event in RFC3339.
    pub timestamp: String,
    /// Type of action performed.
    pub action: LedgerAction,
    /// Result, output, or error detail.
    pub details: String,
    /// Correlation ID linking this evidence to its originating request/turn
    /// (P1.3). Optional and defaulted so pre-existing ledger rows still parse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerAction {
    Started,
    ExecutionFailed,
    ValidationPassed,
    ValidationFailed,
    Retried,
    Paused,
    Resumed,
    Completed,
    ArtifactAdded,
    EphemeralDiffYielded,
    /// An approval gate was granted (P1.3 evidence).
    ApprovalGranted,
    /// A review-gated step is waiting on operator approval before execution.
    AwaitingApproval,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepExecutionResult {
    Completed { step_id: String, output: String },
    Failed { step_id: String, error: String },
    /// A risky step (review-gated) is waiting on operator approval; it did not
    /// execute and the task stays non-terminal so it can resume (P1.3).
    Paused { step_id: String, reason: String },
}

/// Terminal result of a full [`TaskRunner::run`] pass over every step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    /// All steps completed and the final task settled through the owner guard.
    Completed,
    /// A step exhausted its retry policy and the task was settled failed.
    Failed { error: String },
    /// A step is blocked on operator approval; the task stays non-terminal.
    Paused { reason: String },
}

#[derive(Debug, Clone)]
pub struct TaskRunner {
    /// Canonical task row.
    pub task: TaskRecord,
    /// TaskRunner specific spec.
    pub spec: TaskRunnerSpec,
}

impl TaskRunner {
    pub fn new(task: TaskRecord, spec: TaskRunnerSpec) -> Self {
        Self { task, spec }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        self.spec.validate()
    }

    pub fn pause(&mut self, store: &SqliteTaskStore, reason: &str) -> anyhow::Result<()> {
        let index = self.spec.current_step_index;
        anyhow::ensure!(index < self.spec.steps.len(), "task is already completed");
        let step_id = self.spec.steps[index].id.clone();
        let step_status = self.spec.steps[index].status.clone();
        anyhow::ensure!(
            step_status.can_transition_to(&StepStatus::Paused(reason.into())),
            "step {} cannot transition to Paused from {:?}",
            step_id,
            step_status
        );
        self.spec.steps[index].status = StepStatus::Paused(reason.into());
        store.save_task_runner_spec(&self.spec)?;
        self.append_ledger(store, &step_id, LedgerAction::Paused, reason)?;
        Ok(())
    }

    pub fn resume(&mut self, store: &SqliteTaskStore) -> anyhow::Result<()> {
        let index = self.spec.current_step_index;
        anyhow::ensure!(index < self.spec.steps.len(), "task is already completed");
        let step_id = self.spec.steps[index].id.clone();
        let step_status = self.spec.steps[index].status.clone();
        anyhow::ensure!(
            matches!(step_status, StepStatus::Paused(_)),
            "step {} is not paused",
            step_id
        );
        self.spec.steps[index].status = StepStatus::Pending; // Needs to be pending so execute_current_step can run it.
        store.save_task_runner_spec(&self.spec)?;
        self.append_ledger(store, &step_id, LedgerAction::Resumed, "resumed from pause")?;
        Ok(())
    }

    pub fn retry(&mut self, store: &SqliteTaskStore) -> anyhow::Result<()> {
        let index = self.spec.current_step_index;
        anyhow::ensure!(index < self.spec.steps.len(), "task is already completed");
        let step_id = self.spec.steps[index].id.clone();
        let step_status = self.spec.steps[index].status.clone();
        let attempts = self.spec.steps[index].retry_policy.attempts;
        let max_attempts = self.spec.steps[index].retry_policy.max_attempts;
        anyhow::ensure!(
            matches!(step_status, StepStatus::Failed(_)),
            "step {} is not failed",
            step_id
        );
        anyhow::ensure!(
            attempts < max_attempts,
            "step {} has exhausted its retry policy ({} attempts)",
            step_id,
            max_attempts
        );
        self.spec.steps[index].retry_policy.attempts += 1;
        self.spec.steps[index].status = StepStatus::Pending;
        store.save_task_runner_spec(&self.spec)?;
        self.append_ledger(
            store,
            &step_id,
            LedgerAction::Retried,
            &format!("retry attempt {}", attempts + 1),
        )?;
        Ok(())
    }

    /// Execute every remaining step in order, honoring each step's retry policy
    /// and settling the final task through the owner guard exactly once (P1.2).
    ///
    /// `execute`/`validate` are `Fn` (not `FnOnce`) because retry may call them
    /// more than once for a single step.
    pub async fn run<E, V>(
        &mut self,
        store: &SqliteTaskStore,
        execute: E,
        validate_output: V,
    ) -> anyhow::Result<RunOutcome>
    where
        E: Fn(&TaskStep) -> anyhow::Result<String>,
        V: Fn(&TaskStep, &str) -> anyhow::Result<()>,
    {
        self.validate()?;
        loop {
            let index = self.spec.current_step_index;
            if index >= self.spec.steps.len() {
                return Ok(RunOutcome::Completed);
            }
            let step = self.spec.steps[index].clone();
            let max_attempts = step.retry_policy.max_attempts.max(1);
            let mut last_error = String::from("step did not produce a result");
            let mut attempts_used = 0u32;

            loop {
                let result = self
                    .execute_current_step(
                        store,
                        |_| execute(&step),
                        |_, output| validate_output(&step, output),
                    )
                    .await?;
                match result {
                    StepExecutionResult::Completed { .. } => {
                        // `execute_current_step` advanced `current_step_index`.
                        last_error.clear();
                        break;
                    }
                    StepExecutionResult::Failed { error, .. } => {
                        last_error = error;
                        attempts_used += 1;
                        // A retry is only scheduled when another attempt remains,
                        // so a max_attempts == 1 step never records a spurious
                        // `Retried` entry.
                        if attempts_used < max_attempts {
                            self.retry(store)?;
                        } else {
                            break;
                        }
                    }
                    StepExecutionResult::Paused { reason, .. } => {
                        // Blocked on approval: leave the task non-terminal and
                        // stop the pass so it can resume after approval.
                        return Ok(RunOutcome::Paused { reason });
                    }
                }
            }

            if !last_error.is_empty() {
                // The step exhausted its retry policy without succeeding.
                let _ = store
                    .transition_terminal_if_owner(
                        &self.task.id,
                        self.task.owner_pid,
                        &self.task.owner_boot_id,
                        super::task_registry::TaskStatus::Failed,
                        None,
                        Some(last_error.clone()),
                    )
                    .await?;
                return Ok(RunOutcome::Failed { error: last_error });
            }
        }
    }

    /// Record a verified checkpoint for the current step (P1.2). Resumption later
    /// can only restart from a verified checkpoint, never from an unverified one.
    pub fn checkpoint(&mut self, store: &SqliteTaskStore) -> anyhow::Result<()> {
        self.validate()?;
        let step_id = self
            .spec
            .steps
            .get(self.spec.current_step_index)
            .map(|s| s.id.clone())
            .ok_or_else(|| anyhow::anyhow!("task is already completed"))?;
        self.spec.checkpoint = Some(Checkpoint {
            id: format!("checkpoint-{}", uuid::Uuid::new_v4()),
            step_id,
            state_digest: self.state_digest(),
            verified: true,
        });
        store.save_task_runner_spec(&self.spec)?;
        Ok(())
    }

    /// Resume from the last verified checkpoint (P1.2): completed steps before the
    /// checkpoint are preserved and the checkpointed step is reset to run again.
    pub fn resume_from_checkpoint(&mut self, store: &SqliteTaskStore) -> anyhow::Result<()> {
        let checkpoint = self
            .spec
            .checkpoint
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no checkpoint to resume from"))?;
        anyhow::ensure!(
            checkpoint.verified,
            "cannot resume from an unverified checkpoint"
        );
        let index = self
            .spec
            .steps
            .iter()
            .position(|s| s.id == checkpoint.step_id)
            .ok_or_else(|| anyhow::anyhow!("checkpoint step not found in spec"))?;
        anyhow::ensure!(
            self.spec.steps[..index]
                .iter()
                .all(|s| s.status == StepStatus::Completed),
            "steps before the checkpoint must be completed"
        );
        self.spec.current_step_index = index;
        self.spec.steps[index].status = StepStatus::Pending;
        store.save_task_runner_spec(&self.spec)?;
        self.append_ledger(
            store,
            &checkpoint.step_id,
            LedgerAction::Resumed,
            "resumed from verified checkpoint",
        )?;
        Ok(())
    }

    /// Decide the post-restart recovery outcome (P1.2). A verified checkpoint
    /// means `resumed`; anything ambiguous is `needs_review`, never silent success.
    pub fn recovery_decision(&self) -> super::task_registry::RecoveryOutcome {
        match &self.spec.checkpoint {
            Some(checkpoint) if checkpoint.verified => {
                super::task_registry::RecoveryOutcome::Resumed
            }
            _ => super::task_registry::RecoveryOutcome::NeedsReview,
        }
    }

    /// Apply the recovery decision to the canonical task row: `resumed` leaves the
    /// task paused for an operator resume; ambiguity is surfaced as
    /// [`TaskStatus::NeedsReview`] (P1.2).
    pub async fn apply_recovery_decision(
        &self,
        store: &SqliteTaskStore,
    ) -> anyhow::Result<super::task_registry::RecoveryOutcome> {
        let decision = self.recovery_decision();
        let status = match decision {
            super::task_registry::RecoveryOutcome::Resumed => {
                super::task_registry::TaskStatus::Paused
            }
            _ => super::task_registry::TaskStatus::NeedsReview,
        };
        store.update_status(&self.task.id, status, None, None).await?;
        Ok(decision)
    }

    /// A cheap, stable-enough digest of the completed-step boundary used to
    /// fingerprint the checkpoint's resume point.
    fn state_digest(&self) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.spec.current_step_index.hash(&mut hasher);
        for step in &self.spec.steps {
            step.id.hash(&mut hasher);
            step.status.hash(&mut hasher);
        }
        format!("{:016x}", hasher.finish())
    }

    pub async fn execute_current_step<E, V>(
        &mut self,
        store: &SqliteTaskStore,
        execute: E,
        validate_output: V,
    ) -> anyhow::Result<StepExecutionResult>
    where
        E: FnOnce(&TaskStep) -> anyhow::Result<String>,
        V: FnOnce(&TaskStep, &str) -> anyhow::Result<()>,
    {
        self.validate()?;
        let index = self.spec.current_step_index;
        let step = self.spec.steps[index].clone();
        anyhow::ensure!(
            matches!(
                step.status,
                StepStatus::Pending | StepStatus::Failed(_) | StepStatus::Paused(_)
            ),
            "step {} is not executable from {:?}",
            step.id,
            step.status
        );

        // Route risky (review-gated) steps through the approval governance pool
        // BEFORE running the side effect. An unsatisfied review gate parks the
        // step as Paused so the task stays non-terminal and resumable once the
        // operator grants approval via `grant_approval` (P1.3).
        if let Some(required) = self.unsatisfied_review_required(&step) {
            self.spec.steps[index].status = StepStatus::Paused("awaiting operator approval".into());
            store.save_task_runner_spec(&self.spec)?;
            self.append_ledger(
                store,
                &step.id,
                LedgerAction::AwaitingApproval,
                &format!("awaiting {required} approval(s) before execution"),
            )?;
            return Ok(StepExecutionResult::Paused {
                step_id: step.id,
                reason: "awaiting operator approval".into(),
            });
        }

        self.spec.steps[index].status = StepStatus::Executing;
        store.save_task_runner_spec(&self.spec)?;
        self.append_ledger(store, &step.id, LedgerAction::Started, "execution started")?;

        let output = match execute(&step) {
            Ok(output) => output,
            Err(error) => {
                let message = error.to_string();
                self.fail_step(
                    store,
                    index,
                    &step.id,
                    &message,
                    LedgerAction::ExecutionFailed,
                )?;
                return Ok(StepExecutionResult::Failed {
                    step_id: step.id,
                    error: message,
                });
            }
        };
        anyhow::ensure!(
            output.len() <= MAX_TEXT_BYTES,
            "step output exceeds the {} byte bound",
            MAX_TEXT_BYTES
        );
        self.spec.steps[index].output = Some(output.clone());
        self.spec.steps[index].status = StepStatus::Validating;
        store.save_task_runner_spec(&self.spec)?;

        if let Err(error) = validate_output(&step, &output) {
            let message = error.to_string();
            self.spec.steps[index].validation_result = Some(message.clone());
            self.fail_step(
                store,
                index,
                &step.id,
                &message,
                LedgerAction::ValidationFailed,
            )?;
            return Ok(StepExecutionResult::Failed {
                step_id: step.id,
                error: message,
            });
        }

        self.spec.steps[index].validation_result = Some("passed".into());
        self.spec.steps[index].status = StepStatus::Completed;
        self.append_ledger(
            store,
            &step.id,
            LedgerAction::ValidationPassed,
            "validation passed",
        )?;
        self.append_ledger(store, &step.id, LedgerAction::Completed, "step completed")?;
        self.spec.current_step_index += 1;
        store.save_task_runner_spec(&self.spec)?;

        // Stream progress to the parent task (if any) so a supervising parent
        // sees child advancement before the terminal aggregation event (P1.4).
        if let Some(parent_id) = self.task.parent_id.clone() {
            let _ = store
                .record_task_event(
                    &parent_id,
                    "child_progress",
                    &serde_json::json!({
                        "child_id": self.task.id,
                        "completed_steps": self.spec.current_step_index,
                        "total_steps": self.spec.steps.len(),
                    }),
                )
                .await;
        }

        if self.spec.current_step_index == self.spec.steps.len() {
            anyhow::ensure!(
                store
                    .transition_terminal_if_owner(
                        &self.task.id,
                        self.task.owner_pid,
                        &self.task.owner_boot_id,
                        super::task_registry::TaskStatus::Completed,
                        Some(output.clone()),
                        None,
                    )
                    .await?,
                "task owner or lifecycle changed before runner completion"
            );
        }
        Ok(StepExecutionResult::Completed {
            step_id: step.id,
            output,
        })
    }

    /// Returns the required approval count for the first review gate on `step`
    /// that is not yet satisfied by the workflow's approval pool, or `None`
    /// when every review gate is satisfied.
    fn unsatisfied_review_required(&self, step: &TaskStep) -> Option<usize> {
        step.validation_gates.iter().find_map(|gate| match gate {
            ValidationGate::Review { required_approvals } => {
                let satisfied = self
                    .spec
                    .approval_gates
                    .iter()
                    .any(|pool| pool.granted_approvals >= *required_approvals);
                (!satisfied).then_some(*required_approvals)
            }
            _ => None,
        })
    }

    /// Add an artifact to the current workflow with size enforcement and ledger evidence.
    pub fn add_artifact(
        &mut self,
        store: &SqliteTaskStore,
        artifact: Artifact,
    ) -> anyhow::Result<()> {
        self.validate()?;
        anyhow::ensure!(
            self.spec.artifacts.len() < MAX_ARTIFACTS,
            "maximum number of artifacts ({}) reached",
            MAX_ARTIFACTS
        );
        anyhow::ensure!(
            artifact.size_bytes <= 1024 * 1024 * 100, // Hard limit 100MB per artifact for now
            "artifact size exceeds the 100MB bound"
        );
        let current_step_id = self
            .spec
            .steps
            .get(self.spec.current_step_index)
            .map(|s| s.id.clone())
            .unwrap_or_else(|| "global".to_string());

        let artifact_id = artifact.id.clone();
        self.spec.artifacts.push(artifact);
        store.save_task_runner_spec(&self.spec)?;
        self.append_ledger(
            store,
            &current_step_id,
            LedgerAction::ArtifactAdded,
            &format!("artifact {} created", artifact_id),
        )?;
        Ok(())
    }

    /// Yield an intermediate diff as an ephemeral artifact to the task ledger.
    pub fn yield_ephemeral_diff(
        &mut self,
        store: &SqliteTaskStore,
        diff: &str,
    ) -> anyhow::Result<()> {
        self.validate()?;
        let current_step_id = self
            .spec
            .steps
            .get(self.spec.current_step_index)
            .map(|s| s.id.clone())
            .unwrap_or_else(|| "global".to_string());

        self.append_ledger(
            store,
            &current_step_id,
            LedgerAction::EphemeralDiffYielded,
            diff,
        )?;
        Ok(())
    }

    /// Record approval evidence for a gate (P1.3). Increments the gate's granted
    /// count, persists the updated spec, and appends a correlated ledger entry
    /// naming the approver. Idempotent ceiling: a gate can never be granted past
    /// its required approval count.
    pub fn grant_approval(
        &mut self,
        store: &SqliteTaskStore,
        gate_id: &str,
        approver: &str,
    ) -> anyhow::Result<()> {
        self.validate()?;
        anyhow::ensure!(!approver.trim().is_empty(), "approver must not be empty");
        let gate = self
            .spec
            .approval_gates
            .iter_mut()
            .find(|g| g.id == gate_id)
            .ok_or_else(|| anyhow::anyhow!("approval gate {gate_id} not found"))?;
        anyhow::ensure!(
            gate.granted_approvals < gate.required_approvals,
            "approval gate {} already satisfied",
            gate_id
        );
        gate.granted_approvals += 1;
        store.save_task_runner_spec(&self.spec)?;
        let current_step_id = self
            .spec
            .steps
            .get(self.spec.current_step_index)
            .map(|s| s.id.clone())
            .unwrap_or_else(|| "global".to_string());
        self.append_ledger(
            store,
            &current_step_id,
            LedgerAction::ApprovalGranted,
            &format!("gate {gate_id} approved by {approver}"),
        )?;
        Ok(())
    }

    fn append_ledger(
        &self,
        store: &SqliteTaskStore,
        step_id: &str,
        action: LedgerAction,
        details: &str,
    ) -> anyhow::Result<()> {
        store.append_ledger_entry(&LedgerEntry {
            task_id: self.task.id.clone(),
            step_id: step_id.into(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            action,
            details: details.into(),
            correlation_id: self.spec.correlation_id.clone(),
        })
    }

    fn fail_step(
        &mut self,
        store: &SqliteTaskStore,
        index: usize,
        step_id: &str,
        message: &str,
        action: LedgerAction,
    ) -> anyhow::Result<()> {
        self.spec.steps[index].status = StepStatus::Failed(message.into());
        self.spec.steps[index].validation_result = Some(message.into());
        self.append_ledger(store, step_id, action, message)?;
        store.save_task_runner_spec(&self.spec)
    }

    /// Evaluates all validation gates (Test, Lint, Build) autonomously in a subprocess.
    /// Review gates are not evaluated here — they are enforced by
    /// [`TaskRunner::execute_current_step`] against the workflow's approval pool
    /// (a risky step parks as `Paused` until the operator grants approval).
    pub async fn evaluate_validation_gates(
        &self,
        step_index: usize,
        workspace_dir: Option<&std::path::Path>,
    ) -> anyhow::Result<()> {
        let step = &self.spec.steps[step_index];
        for gate in &step.validation_gates {
            match gate {
                ValidationGate::Test { command }
                | ValidationGate::Lint { command }
                | ValidationGate::Build { command } => {
                    let mut cmd = platform_shell(command);
                    if let Some(dir) = workspace_dir {
                        cmd.current_dir(dir);
                    }
                    let output = cmd.output().await.map_err(|e| {
                        anyhow::anyhow!("failed to spawn validation command: {}: {}", command, e)
                    })?;
                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        anyhow::bail!("validation failed ({}): {}\n{}", command, stdout, stderr);
                    }
                }
                ValidationGate::Review { .. } => {
                    // Enforced by `execute_current_step` against the approval
                    // pool; not a subprocess gate.
                }
            }
        }
        Ok(())
    }
}

impl TaskRunnerSpec {
    /// Normalize an on-disk spec to the current schema version (P1.1). A spec
    /// from a newer runtime is rejected (downgrade is unsafe); older specs are
    /// stamped to the current version after validation. Real field migration
    /// becomes necessary only once a v2 contract exists.
    pub fn migrate(mut self) -> anyhow::Result<Self> {
        anyhow::ensure!(
            self.schema_version <= TASK_RUNNER_SCHEMA_VERSION,
            "cannot downgrade task runner schema {} to {}",
            self.schema_version,
            TASK_RUNNER_SCHEMA_VERSION
        );
        self.schema_version = TASK_RUNNER_SCHEMA_VERSION;
        self.validate()?;
        Ok(self)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema_version <= TASK_RUNNER_SCHEMA_VERSION,
            "unsupported task runner schema version {}",
            self.schema_version
        );
        validate_text("task_id", &self.task_id, MAX_ID_BYTES)?;
        anyhow::ensure!(!self.steps.is_empty(), "task runner must contain a step");
        anyhow::ensure!(
            self.steps.len() <= MAX_STEPS,
            "task runner has too many steps"
        );
        anyhow::ensure!(
            self.current_step_index <= self.steps.len(),
            "current step index is outside the task runner"
        );
        if self.current_step_index == self.steps.len() {
            anyhow::ensure!(
                self.steps
                    .iter()
                    .all(|step| step.status == StepStatus::Completed),
                "terminal task runner must have all steps completed"
            );
        }

        let mut step_ids = std::collections::HashSet::new();
        for step in &self.steps {
            validate_text("step id", &step.id, MAX_ID_BYTES)?;
            validate_text("step name", &step.name, MAX_TEXT_BYTES)?;
            validate_text("step description", &step.description, MAX_TEXT_BYTES)?;
            anyhow::ensure!(
                step_ids.insert(&step.id),
                "duplicate task step id {}",
                step.id
            );
            if let Some(output) = &step.output {
                validate_text("step output", output, MAX_TEXT_BYTES)?;
            }
            if let Some(validation_result) = &step.validation_result {
                validate_text("validation result", validation_result, MAX_TEXT_BYTES)?;
            }
            anyhow::ensure!(
                step.retry_policy.max_attempts > 0
                    && step.retry_policy.attempts <= step.retry_policy.max_attempts,
                "invalid retry policy for step {}",
                step.id
            );
            for gate in &step.validation_gates {
                match gate {
                    ValidationGate::Test { command }
                    | ValidationGate::Lint { command }
                    | ValidationGate::Build { command } => {
                        validate_text("validation command", command, MAX_TEXT_BYTES)?;
                    }
                    ValidationGate::Review { required_approvals } => {
                        anyhow::ensure!(*required_approvals > 0, "review gate requires approval");
                    }
                }
            }
        }

        anyhow::ensure!(
            self.artifacts.len() <= MAX_ARTIFACTS,
            "too many task artifacts"
        );
        for artifact in &self.artifacts {
            validate_text("artifact id", &artifact.id, MAX_ID_BYTES)?;
            validate_text("artifact digest", &artifact.digest, MAX_TEXT_BYTES)?;
        }
        if let Some(checkpoint) = &self.checkpoint {
            validate_text("checkpoint id", &checkpoint.id, MAX_ID_BYTES)?;
            validate_text("checkpoint step id", &checkpoint.step_id, MAX_ID_BYTES)?;
            anyhow::ensure!(
                checkpoint.verified,
                "task runner checkpoint must be verified before resume"
            );
        }
        for gate in &self.approval_gates {
            validate_text("approval gate id", &gate.id, MAX_ID_BYTES)?;
            anyhow::ensure!(
                gate.required_approvals > 0 && gate.granted_approvals <= gate.required_approvals,
                "invalid approval gate {}",
                gate.id
            );
        }
        if let Some(correlation_id) = &self.correlation_id {
            validate_text("correlation id", correlation_id, MAX_ID_BYTES)?;
        }
        Ok(())
    }
}

fn validate_text(name: &str, value: &str, max_bytes: usize) -> anyhow::Result<()> {
    anyhow::ensure!(!value.is_empty(), "{name} must not be empty");
    anyhow::ensure!(
        value.len() <= max_bytes,
        "{name} exceeds the {} byte bound",
        max_bytes
    );
    Ok(())
}

/// Build a shell command for a validation gate that runs on the host OS:
/// `cmd /C` on Windows (there is no `sh`), `sh -c` elsewhere.
fn platform_shell(command: &str) -> tokio::process::Command {
    #[cfg(windows)]
    {
        let mut cmd = tokio::process::Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    }
    #[cfg(not(windows))]
    {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_plane::task_registry::{CancellationState, TaskKind, TaskStatus};

    fn spec() -> TaskRunnerSpec {
        TaskRunnerSpec {
            schema_version: TASK_RUNNER_SCHEMA_VERSION,
            task_id: "task-1".into(),
            steps: vec![TaskStep {
                id: "step-1".into(),
                name: "Plan".into(),
                description: "plan the work".into(),
                validation_gates: vec![ValidationGate::Review {
                    required_approvals: 1,
                }],
                retry_policy: RetryPolicy::default(),
                status: StepStatus::Pending,
                output: None,
                validation_result: None,
            }],
            current_step_index: 0,
            checkpoint: None,
            artifacts: Vec::new(),
            approval_gates: vec![ApprovalGate {
                id: "approval-1".into(),
                required_approvals: 1,
                granted_approvals: 0,
            }],
            correlation_id: None,
        }
    }

    #[test]
    fn missing_schema_version_uses_current_contract() {
        let decoded: TaskRunnerSpec = serde_json::from_value(serde_json::json!({
            "task_id": "task-1",
            "steps": [{
                "id": "step-1",
                "name": "Plan",
                "description": "plan the work"
            }]
        }))
        .unwrap();
        assert_eq!(decoded.schema_version, TASK_RUNNER_SCHEMA_VERSION);
        decoded.validate().unwrap();
    }

    #[test]
    fn validation_rejects_unverified_checkpoint_and_duplicate_steps() {
        let mut invalid = spec();
        invalid.checkpoint = Some(Checkpoint {
            id: "checkpoint-1".into(),
            step_id: "step-1".into(),
            state_digest: "sha256:test".into(),
            verified: false,
        });
        assert!(invalid.validate().is_err());

        let mut duplicate = spec();
        duplicate.steps.push(duplicate.steps[0].clone());
        assert!(duplicate.validate().is_err());
    }

    #[test]
    fn step_transitions_are_explicit_and_fail_closed() {
        assert!(StepStatus::Pending.can_transition_to(&StepStatus::Executing));
        assert!(StepStatus::Executing.can_transition_to(&StepStatus::Validating));
        assert!(StepStatus::Failed("retry".into()).can_transition_to(&StepStatus::Executing));
        assert!(!StepStatus::Pending.can_transition_to(&StepStatus::Completed));
        assert!(!StepStatus::Completed.can_transition_to(&StepStatus::Executing));
    }

    #[tokio::test]
    async fn executor_validates_output_and_settles_the_final_task_once() {
        let store = SqliteTaskStore::new_in_memory().unwrap();
        let task = _task_record_fixture();
        store.create(task.clone()).await.unwrap();
        let mut spec = spec();
        spec.steps[0].validation_gates = Vec::new();
        spec.approval_gates = Vec::new();
        let mut runner = TaskRunner::new(task, spec);

        let result = runner
            .execute_current_step(
                &store,
                |_| Ok("verified output".into()),
                |_, output| {
                    anyhow::ensure!(output == "verified output", "unexpected output");
                    Ok(())
                },
            )
            .await
            .unwrap();

        assert_eq!(
            result,
            StepExecutionResult::Completed {
                step_id: "step-1".into(),
                output: "verified output".into()
            }
        );
        assert_eq!(
            store.get("task-1").await.unwrap().unwrap().status,
            TaskStatus::Completed
        );
        assert_eq!(store.get_ledger_entries("task-1").unwrap().len(), 3);
    }

    #[tokio::test]
    async fn failed_validation_keeps_task_non_terminal_and_records_failure() {
        let store = SqliteTaskStore::new_in_memory().unwrap();
        let task = _task_record_fixture();
        store.create(task.clone()).await.unwrap();
        let mut spec = spec();
        spec.steps[0].validation_gates = Vec::new();
        spec.approval_gates = Vec::new();
        let mut runner = TaskRunner::new(task, spec);

        let result = runner
            .execute_current_step(
                &store,
                |_| Ok("untrusted output".into()),
                |_, _| anyhow::bail!("required gate failed"),
            )
            .await
            .unwrap();

        assert!(matches!(result, StepExecutionResult::Failed { .. }));
        assert_eq!(
            store.get("task-1").await.unwrap().unwrap().status,
            TaskStatus::Running
        );
        assert_eq!(store.get_ledger_entries("task-1").unwrap().len(), 2);
    }

    #[tokio::test]
    async fn artifact_creation_is_enforced_and_recorded() {
        let store = SqliteTaskStore::new_in_memory().unwrap();
        let task = _task_record_fixture();
        store.create(task.clone()).await.unwrap();
        let spec = spec();
        let mut runner = TaskRunner::new(task, spec);

        let artifact = Artifact {
            id: "artifact-1".into(),
            digest: "sha256:abc".into(),
            size_bytes: 1024,
        };

        runner.add_artifact(&store, artifact.clone()).unwrap();

        let updated_spec = store.get_task_runner_spec("task-1").unwrap().unwrap();
        assert_eq!(updated_spec.artifacts.len(), 1);
        assert_eq!(updated_spec.artifacts[0].id, "artifact-1");

        let ledger = store.get_ledger_entries("task-1").unwrap();
        assert_eq!(ledger.len(), 1);
        assert!(matches!(ledger[0].action, LedgerAction::ArtifactAdded));

        let large_artifact = Artifact {
            id: "artifact-2".into(),
            digest: "sha256:def".into(),
            size_bytes: 1024 * 1024 * 200, // 200MB (exceeds 100MB bound)
        };
        assert!(runner.add_artifact(&store, large_artifact).is_err());
    }

    fn multi_step_spec(n: usize) -> TaskRunnerSpec {
        let steps = (0..n)
            .map(|i| TaskStep {
                id: format!("step-{}", i),
                name: format!("Step {}", i),
                description: format!("do step {}", i),
                validation_gates: Vec::new(),
                retry_policy: RetryPolicy::default(),
                status: StepStatus::Pending,
                output: None,
                validation_result: None,
            })
            .collect();
        TaskRunnerSpec {
            schema_version: TASK_RUNNER_SCHEMA_VERSION,
            task_id: "task-1".into(),
            steps,
            current_step_index: 0,
            checkpoint: None,
            artifacts: Vec::new(),
            approval_gates: Vec::new(),
            correlation_id: None,
        }
    }

    #[tokio::test]
    async fn run_completes_all_steps_and_settles_task_once() {
        let store = SqliteTaskStore::new_in_memory().unwrap();
        let task = _task_record_fixture();
        store.create(task.clone()).await.unwrap();
        let mut runner = TaskRunner::new(task, multi_step_spec(3));

        let outcome = runner
            .run(&store, |_| Ok("out".into()), |_, _| Ok(()))
            .await
            .unwrap();

        assert_eq!(outcome, RunOutcome::Completed);
        assert_eq!(
            store.get("task-1").await.unwrap().unwrap().status,
            TaskStatus::Completed
        );
        // 3 steps × (Started + ValidationPassed + Completed) ledger entries.
        assert_eq!(store.get_ledger_entries("task-1").unwrap().len(), 9);
    }

    #[tokio::test]
    async fn run_retries_then_settles_failed_after_exhaustion() {
        let store = SqliteTaskStore::new_in_memory().unwrap();
        let task = _task_record_fixture();
        store.create(task.clone()).await.unwrap();
        let mut spec = multi_step_spec(1);
        spec.steps[0].retry_policy.max_attempts = 2;
        let mut runner = TaskRunner::new(task, spec);

        let outcome = runner
            .run(&store, |_| Ok("bad".into()), |_, _| anyhow::bail!("nope"))
            .await
            .unwrap();

        assert_eq!(
            outcome,
            RunOutcome::Failed {
                error: "nope".into()
            }
        );
        let rec = store.get("task-1").await.unwrap().unwrap();
        assert_eq!(rec.status, TaskStatus::Failed);
        assert!(rec.finished_at.is_some());
    }

    #[tokio::test]
    async fn checkpoint_and_resume_only_from_verified_checkpoint() {
        let store = SqliteTaskStore::new_in_memory().unwrap();
        let task = _task_record_fixture();
        store.create(task.clone()).await.unwrap();
        let spec = multi_step_spec(2);
        let mut runner = TaskRunner::new(task.clone(), spec);

        // Complete step 0 (advances index to 1).
        runner
            .execute_current_step(&store, |_| Ok("a".into()), |_, _| Ok(()))
            .await
            .unwrap();
        runner.checkpoint(&store).unwrap();
        assert!(runner.spec.checkpoint.as_ref().unwrap().verified);

        // Simulate a restart by rebuilding the runner from the persisted spec.
        let persisted = store.get_task_runner_spec("task-1").unwrap().unwrap();
        let mut recovered = TaskRunner::new(task, persisted);
        assert_eq!(
            recovered.recovery_decision(),
            crate::control_plane::task_registry::RecoveryOutcome::Resumed
        );
        recovered.resume_from_checkpoint(&store).unwrap();
        assert_eq!(recovered.spec.current_step_index, 1);
        assert_eq!(recovered.spec.steps[0].status, StepStatus::Completed);

        // An unverified checkpoint must be rejected.
        recovered.spec.checkpoint.as_mut().unwrap().verified = false;
        assert!(recovered.resume_from_checkpoint(&store).is_err());
    }

    #[tokio::test]
    async fn ambiguous_recovery_marks_needs_review() {
        let store = SqliteTaskStore::new_in_memory().unwrap();
        let task = _task_record_fixture();
        store.create(task.clone()).await.unwrap();
        let runner = TaskRunner::new(task, multi_step_spec(2));

        assert_eq!(
            runner.recovery_decision(),
            crate::control_plane::task_registry::RecoveryOutcome::NeedsReview
        );
        runner.apply_recovery_decision(&store).await.unwrap();
        assert_eq!(
            store.get("task-1").await.unwrap().unwrap().status,
            TaskStatus::NeedsReview
        );
    }

    #[test]
    fn migrate_stamps_version_and_rejects_downgrade() {
        let migrated = spec().migrate().unwrap();
        assert_eq!(migrated.schema_version, TASK_RUNNER_SCHEMA_VERSION);

        let mut newer = spec();
        newer.schema_version = TASK_RUNNER_SCHEMA_VERSION + 1;
        assert!(newer.migrate().is_err());
    }

    // ── P1.3 — approval + validation evidence with correlation IDs ──

    #[tokio::test]
    async fn approval_evidence_is_persisted_with_correlation_id() {
        let store = SqliteTaskStore::new_in_memory().unwrap();
        let task = _task_record_fixture();
        store.create(task.clone()).await.unwrap();
        let mut spec = spec();
        spec.correlation_id = Some("corr-123".into());
        let mut runner = TaskRunner::new(task, spec);

        runner.grant_approval(&store, "approval-1", "operator").unwrap();

        let persisted = store.get_task_runner_spec("task-1").unwrap().unwrap();
        assert_eq!(persisted.approval_gates[0].granted_approvals, 1);

        let ledger = store.get_ledger_entries("task-1").unwrap();
        assert_eq!(ledger.len(), 1);
        assert!(matches!(ledger[0].action, LedgerAction::ApprovalGranted));
        assert_eq!(ledger[0].correlation_id.as_deref(), Some("corr-123"));

        // A gate cannot be granted past its required approval count.
        assert!(runner.grant_approval(&store, "approval-1", "operator").is_err());
        // Unknown gate is refused.
        assert!(runner.grant_approval(&store, "missing", "operator").is_err());
    }

    // ── P1.6 — boundary tests ──

    /// Retry must not duplicate an external side effect. The framework-level
    /// guarantee is the idempotency key on task creation: a producer registers
    /// its side effect with a stable `idem_key`, so a retried step that
    /// re-registers the same effect is refused rather than duplicated.
    #[tokio::test]
    async fn idempotency_key_prevents_duplicate_side_effect() {
        let store = SqliteTaskStore::new_in_memory().unwrap();
        let mut effect = _task_record_fixture();
        effect.id = "effect-1".into();
        effect.idem_key = Some("side-effect-1".into());
        effect.principal_id = Some("user-1".into());
        effect.session_key = Some("session-1".into());

        // First registration succeeds; the retry's duplicate is refused.
        store.create(effect.clone()).await.unwrap();
        assert!(store.create(effect).await.is_err());
    }

    /// A retried step is re-executed until it succeeds or its policy is
    /// exhausted, and the retries are recorded in the ledger.
    #[tokio::test]
    async fn run_retries_failed_step_until_success() {
        let store = SqliteTaskStore::new_in_memory().unwrap();
        let task = _task_record_fixture();
        store.create(task.clone()).await.unwrap();
        let mut spec = multi_step_spec(1);
        spec.steps[0].retry_policy.max_attempts = 3;
        let mut runner = TaskRunner::new(task, spec);

        let attempts = std::cell::Cell::new(0u32);
        let outcome = runner
            .run(
                &store,
                |_| {
                    let n = attempts.get();
                    attempts.set(n + 1);
                    if n < 1 {
                        anyhow::bail!("transient failure")
                    }
                    Ok("done".into())
                },
                |_, _| Ok(()),
            )
            .await
            .unwrap();

        assert_eq!(outcome, RunOutcome::Completed);
        assert_eq!(attempts.get(), 2, "step must be re-executed after failure");
        let ledger = store.get_ledger_entries("task-1").unwrap();
        assert!(
            ledger.iter().any(|e| matches!(e.action, LedgerAction::Retried)),
            "retry must be recorded in the ledger"
        );
    }

    /// A parent must receive exactly one terminal child result even when the
    /// aggregation is attempted more than once.
    #[tokio::test]
    async fn parent_receives_one_terminal_child_result() {
        let store = SqliteTaskStore::new_in_memory().unwrap();
        let mut parent = _task_record_fixture();
        parent.id = "parent-1".into();
        store.create(parent.clone()).await.unwrap();
        let mut child = _task_record_fixture();
        child.id = "child-1".into();
        child.parent_id = Some("parent-1".into());
        child.depth = 1;
        store.create(child.clone()).await.unwrap();

        assert!(
            store
                .aggregate_child_result(
                    "parent-1",
                    "child-1",
                    TaskStatus::Completed,
                    Some("result".into()),
                )
                .await
                .unwrap()
        );
        // A second aggregation is a no-op (exactly-once).
        assert!(
            !store
                .aggregate_child_result(
                    "parent-1",
                    "child-1",
                    TaskStatus::Completed,
                    Some("result".into()),
                )
                .await
                .unwrap()
        );
        // A non-child cannot be aggregated into this parent.
        assert!(
            !store
                .aggregate_child_result("parent-1", "task-1", TaskStatus::Completed, None)
                .await
                .unwrap()
        );
    }

    /// A review-gated (risky) step must not execute until approval is granted;
    /// it parks as Paused and resumes to completion after `grant_approval`.
    #[tokio::test]
    async fn review_gate_parks_step_until_approval_then_completes() {
        let store = SqliteTaskStore::new_in_memory().unwrap();
        let task = _task_record_fixture();
        store.create(task.clone()).await.unwrap();
        // spec() carries a Review{required_approvals: 1} gate and an unsatisfied
        // approval pool entry ("approval-1", 0/1).
        let mut runner = TaskRunner::new(task.clone(), spec());

        let outcome = runner
            .run(&store, |_| Ok("ran".into()), |_, _| Ok(()))
            .await
            .unwrap();
        assert_eq!(
            outcome,
            RunOutcome::Paused {
                reason: "awaiting operator approval".into()
            }
        );
        // The side effect did not run and the task is not terminal.
        assert_eq!(
            runner.spec.steps[0].status,
            StepStatus::Paused("awaiting operator approval".into())
        );
        assert_eq!(
            store.get("task-1").await.unwrap().unwrap().status,
            TaskStatus::Running
        );

        // Grant approval, resume, and re-run to completion.
        runner.grant_approval(&store, "approval-1", "operator").unwrap();
        runner.resume(&store).unwrap();
        let outcome = runner
            .run(&store, |_| Ok("ran".into()), |_, _| Ok(()))
            .await
            .unwrap();
        assert_eq!(outcome, RunOutcome::Completed);
        assert_eq!(
            store.get("task-1").await.unwrap().unwrap().status,
            TaskStatus::Completed
        );
    }

    /// A child workflow streams step progress to its parent task before settling.
    #[tokio::test]
    async fn child_progress_streams_to_parent() {
        let store = SqliteTaskStore::new_in_memory().unwrap();
        let mut parent = _task_record_fixture();
        parent.id = "parent-1".into();
        store.create(parent).await.unwrap();
        let mut child = _task_record_fixture();
        child.id = "child-1".into();
        child.parent_id = Some("parent-1".into());
        child.depth = 1;
        store.create(child.clone()).await.unwrap();

        let mut spec = multi_step_spec(2);
        spec.task_id = child.id.clone();
        let mut runner = TaskRunner::new(child, spec);
        let outcome = runner
            .run(&store, |_| Ok("ok".into()), |_, _| Ok(()))
            .await
            .unwrap();
        assert_eq!(outcome, RunOutcome::Completed);

        let events = store.list_task_events("parent-1", 50, 0).await.unwrap();
        let progress: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == "child_progress")
            .collect();
        assert_eq!(progress.len(), 2, "one progress event per completed step");
        let mut completed: Vec<i64> = progress
            .iter()
            .map(|e| e.payload["completed_steps"].as_i64().unwrap())
            .collect();
        completed.sort();
        assert_eq!(completed, vec![1, 2]);
    }

    /// Build/test/lint gate adapters run the configured command and fail closed
    /// when it exits non-zero.
    #[tokio::test]
    async fn validation_gate_adapters_run_commands_and_fail_closed() {
        let task = _task_record_fixture();
        let ok = if cfg!(windows) { "exit 0" } else { "true" };
        let fail = if cfg!(windows) { "exit 1" } else { "false" };

        let mut ok_spec = multi_step_spec(1);
        ok_spec.steps[0].validation_gates = vec![ValidationGate::Build {
            command: ok.into(),
        }];
        assert!(
            TaskRunner::new(task.clone(), ok_spec)
                .evaluate_validation_gates(0, None)
                .await
                .is_ok()
        );

        let mut fail_spec = multi_step_spec(1);
        fail_spec.steps[0].validation_gates = vec![ValidationGate::Test {
            command: fail.into(),
        }];
        assert!(
            TaskRunner::new(task, fail_spec)
                .evaluate_validation_gates(0, None)
                .await
                .is_err()
        );
    }

    #[allow(dead_code)]
    fn _task_record_fixture() -> TaskRecord {
        TaskRecord {
            id: "task-1".into(),
            kind: TaskKind::Workflow,
            agent: "main".into(),
            status: TaskStatus::Running,
            owner_pid: 1,
            owner_boot_id: "boot-1".into(),
            heartbeat_at: None,
            depth: 0,
            parent_id: None,
            originator_route: None,
            delivered: false,
            idem_key: None,
            principal_id: None,
            session_key: None,
            workspace: None,
            cancellation_state: CancellationState::None,
            checkpoint_id: None,
            recovery_outcome: Default::default(),
            started_at: "2026-01-01T00:00:00Z".into(),
            finished_at: None,
        }
    }
}
