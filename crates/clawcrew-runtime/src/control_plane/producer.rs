//! Producer admission/settlement seam for background work that owns a separate
//! trigger store (cron, SOP, A2A).
//!
//! Producers keep their own trigger/protocol stores, but task *lifecycle* is a
//! projection of the one canonical control plane: register a task at admission
//! and settle it through the owner-checked transition. Producer-specific state
//! must live in an extension projection (like the goal extension table), never
//! as a second lifecycle authority.

use super::task_registry::{TaskKind, TaskRecord, TaskRegistry, TaskStatus};

/// Kinds that may be registered through this seam. Delegate/Subagent/Goal/
/// Workflow/PeerInbox have their own producers and are rejected here.
pub fn validate_producer_kind(kind: TaskKind) -> anyhow::Result<()> {
    anyhow::ensure!(
        matches!(kind, TaskKind::Cron | TaskKind::Sop | TaskKind::A2a | TaskKind::RemoteTurn),
        "kind {kind:?} is not a registerable producer kind"
    );
    Ok(())
}

/// Register a producer task as `queued`, owned by the current process/boot.
pub async fn register_producer_task(
    store: &dyn TaskRegistry,
    boot_id: &str,
    kind: TaskKind,
    agent: &str,
    id: &str,
    idem_key: Option<String>,
) -> anyhow::Result<()> {
    validate_producer_kind(kind)?;
    anyhow::ensure!(!id.trim().is_empty(), "producer task id must not be empty");
    anyhow::ensure!(!agent.trim().is_empty(), "producer task agent must not be empty");
    store
        .create(TaskRecord {
            id: id.to_string(),
            kind,
            agent: agent.to_string(),
            status: TaskStatus::Queued,
            owner_pid: std::process::id(),
            owner_boot_id: boot_id.to_string(),
            heartbeat_at: None,
            depth: 0,
            parent_id: None,
            originator_route: None,
            delivered: false,
            idem_key,
            principal_id: None,
            session_key: None,
            workspace: None,
            cancellation_state: Default::default(),
            checkpoint_id: None,
            recovery_outcome: Default::default(),
            started_at: chrono::Utc::now().to_rfc3339(),
            finished_at: None,
        })
        .await
}

/// Settle a producer task through the canonical owner-checked transition.
/// Returns `true` only for the owner that won the terminal transition.
pub async fn settle_producer_task(
    store: &dyn TaskRegistry,
    task_id: &str,
    owner_pid: u32,
    owner_boot_id: &str,
    status: TaskStatus,
    output: Option<String>,
    error: Option<String>,
) -> anyhow::Result<bool> {
    store
        .transition_terminal_if_owner(
            task_id,
            owner_pid,
            owner_boot_id,
            status,
            output,
            error,
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_plane::task_store_sqlite::SqliteTaskStore;

    #[tokio::test]
    async fn cron_producer_registers_and_settles_through_the_canonical_store() {
        let store = SqliteTaskStore::new_in_memory().unwrap();
        register_producer_task(
            &store,
            "boot-1",
            TaskKind::Cron,
            "main",
            "cron-1",
            Some("job-1".into()),
        )
        .await
        .unwrap();

        let rec = store.get("cron-1").await.unwrap().unwrap();
        assert_eq!(rec.kind, TaskKind::Cron);
        assert_eq!(rec.status, TaskStatus::Queued);
        assert_eq!(rec.idem_key.as_deref(), Some("job-1"));

        assert!(
            settle_producer_task(
                &store,
                "cron-1",
                rec.owner_pid,
                &rec.owner_boot_id,
                TaskStatus::Completed,
                Some("done".into()),
                None,
            )
            .await
            .unwrap()
        );
        assert_eq!(
            store.get("cron-1").await.unwrap().unwrap().status,
            TaskStatus::Completed
        );
        // A second settlement cannot win.
        assert!(
            !settle_producer_task(
                &store,
                "cron-1",
                rec.owner_pid,
                &rec.owner_boot_id,
                TaskStatus::Failed,
                None,
                Some("late".into()),
            )
            .await
            .unwrap()
        );
    }

    #[tokio::test]
    async fn non_producer_kind_is_rejected() {
        let store = SqliteTaskStore::new_in_memory().unwrap();
        assert!(
            register_producer_task(&store, "boot", TaskKind::Delegate, "main", "d1", None)
                .await
                .is_err()
        );
        assert!(register_producer_task(&store, "boot", TaskKind::Sop, "", "s1", None)
            .await
            .is_err());
    }

    #[test]
    fn validate_accepts_only_cron_sop_a2a_remoteturn() {
        assert!(validate_producer_kind(TaskKind::Cron).is_ok());
        assert!(validate_producer_kind(TaskKind::Sop).is_ok());
        assert!(validate_producer_kind(TaskKind::A2a).is_ok());
        assert!(validate_producer_kind(TaskKind::RemoteTurn).is_ok());
        assert!(validate_producer_kind(TaskKind::Delegate).is_err());
        assert!(validate_producer_kind(TaskKind::Workflow).is_err());
    }
}