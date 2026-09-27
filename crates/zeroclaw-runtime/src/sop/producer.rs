//! Project SOP run lifecycle onto the canonical control plane (P0.1).
//!
//! SOP runs keep their own durable engine/store. This projects each run as a
//! `TaskKind::Sop` task: registered when the run is started/advanced and
//! settled when the run reaches a terminal action. Non-fatal: a missing control
//! plane never affects SOP execution.

use crate::control_plane::{control_plane, producer, ControlPlaneHandle, TaskKind, TaskStatus};
use crate::sop::types::SopRunAction;

/// Stable canonical task id for a run.
fn sop_task_id(run_id: &str) -> String {
    format!("sop:{run_id}")
}

/// Map an action to `(run_id, terminal_status)`.
fn classify(action: &SopRunAction) -> (&str, Option<TaskStatus>) {
    match action {
        SopRunAction::Completed { run_id, .. } => (run_id, Some(TaskStatus::Completed)),
        SopRunAction::Cancelled { run_id, .. } => (run_id, Some(TaskStatus::Cancelled)),
        SopRunAction::Failed { run_id, .. } => (run_id, Some(TaskStatus::Failed)),
        SopRunAction::ExecuteStep { run_id, .. }
        | SopRunAction::WaitApproval { run_id, .. }
        | SopRunAction::DeterministicStep { run_id, .. }
        | SopRunAction::CheckpointWait { run_id, .. }
        | SopRunAction::Pending { run_id, .. } => (run_id, None),
    }
}

/// Project an action using the process-global control plane (no-op when absent).
pub async fn project_sop_action(action: &SopRunAction, agent_label: &str) {
    project_sop_action_with(control_plane(), action, agent_label).await;
}

/// Project an action onto an explicit control plane handle (testable).
pub async fn project_sop_action_with(
    control_plane: Option<&ControlPlaneHandle>,
    action: &SopRunAction,
    agent_label: &str,
) {
    let Some(control_plane) = control_plane else {
        return;
    };
    let (run_id, terminal) = classify(action);
    let task_id = sop_task_id(run_id);

    let existing = control_plane.store.get(&task_id).await.ok().flatten();
    if existing.is_none() {
        let _ = producer::register_producer_task(
            control_plane.store.as_ref(),
            &control_plane.boot_id,
            TaskKind::Sop,
            agent_label,
            &task_id,
            None,
        )
        .await;
    }

    if let Some(status) = terminal {
        // Settle with the recorded owner identity so a restart between
        // registration and completion still settles the right row.
        if let Ok(Some(record)) = control_plane.store.get(&task_id).await
            && !record.status.is_terminal()
        {
            let _ = producer::settle_producer_task(
                control_plane.store.as_ref(),
                &task_id,
                record.owner_pid,
                &record.owner_boot_id,
                status,
                None,
                None,
            )
            .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(run_id: &str, terminal: Option<TaskStatus>) -> SopRunAction {
        match terminal {
            Some(TaskStatus::Completed) => SopRunAction::Completed {
                run_id: run_id.into(),
                sop_name: "s".into(),
            },
            Some(TaskStatus::Cancelled) => SopRunAction::Cancelled {
                run_id: run_id.into(),
                sop_name: "s".into(),
            },
            Some(TaskStatus::Failed) => SopRunAction::Failed {
                run_id: run_id.into(),
                sop_name: "s".into(),
                reason: "boom".into(),
            },
            _ => SopRunAction::Pending {
                run_id: run_id.into(),
                sop_name: "s".into(),
                step: 1,
                reason: "wait".into(),
            },
        }
    }

    #[tokio::test]
    async fn sop_run_projects_register_then_settle() {
        let dir = tempfile::tempdir().unwrap();
        let cp = ControlPlaneHandle::open(dir.path()).unwrap();

        // No control plane -> no-op.
        project_sop_action_with(None, &action("run-1", None), "sop").await;

        // Non-terminal action registers a Sop task.
        project_sop_action_with(Some(&cp), &action("run-1", None), "sop").await;
        let task_id = sop_task_id("run-1");
        let rec = cp.store.get(&task_id).await.unwrap().unwrap();
        assert_eq!(rec.kind, TaskKind::Sop);
        assert_eq!(rec.status, TaskStatus::Queued);

        // Terminal action settles it.
        project_sop_action_with(Some(&cp), &action("run-1", Some(TaskStatus::Completed)), "sop")
            .await;
        assert_eq!(
            cp.store.get(&task_id).await.unwrap().unwrap().status,
            TaskStatus::Completed
        );
    }

    #[tokio::test]
    async fn terminal_action_without_prior_registration_registers_then_settles() {
        let dir = tempfile::tempdir().unwrap();
        let cp = ControlPlaneHandle::open(dir.path()).unwrap();
        project_sop_action_with(Some(&cp), &action("run-2", Some(TaskStatus::Failed)), "sop").await;
        assert_eq!(
            cp.store.get(&sop_task_id("run-2")).await.unwrap().unwrap().status,
            TaskStatus::Failed
        );
    }

    #[test]
    fn classify_maps_terminal_variants() {
        assert_eq!(classify(&action("r", None)).1, None);
        assert_eq!(
            classify(&action("r", Some(TaskStatus::Completed))).1,
            Some(TaskStatus::Completed)
        );
        assert_eq!(
            classify(&action("r", Some(TaskStatus::Cancelled))).1,
            Some(TaskStatus::Cancelled)
        );
    }
}