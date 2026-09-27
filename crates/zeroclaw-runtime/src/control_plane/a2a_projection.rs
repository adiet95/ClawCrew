//! Project outbound A2A run events onto the canonical control plane (P0.1).
//!
//! The A2A tools live in `zeroclaw-tools`, below the runtime control plane.
//! They emit [`A2aRunEvent`]s through the `zeroclaw_api::a2a_observer` seam; the
//! observer installed here projects each event onto `TaskRegistry` as a
//! `TaskKind::A2a` task (registered on first sight, settled when terminal).

use std::sync::Arc;

use zeroclaw_api::a2a_observer::{A2aRunEvent, A2aRunObserver, set_a2a_run_observer};
use zeroclaw_api::a2a_wire::TaskState;

use super::{ControlPlaneHandle, TaskKind, TaskStatus, control_plane, producer};

/// Stable canonical task id for an outbound A2A run.
fn a2a_task_id(peer: &str, remote_task_id: &str) -> String {
    format!("a2a:{peer}:{remote_task_id}")
}

/// Install the process-wide observer that projects A2A run events. Call once at
/// daemon boot after the control plane is initialized.
pub(crate) fn install() {
    let _ = set_a2a_run_observer(Arc::new(ProjectingObserver));
}

struct ProjectingObserver;

impl A2aRunObserver for ProjectingObserver {
    fn on_run_event(&self, event: &A2aRunEvent) {
        let Some(control_plane) = control_plane() else {
            return;
        };
        let control_plane = control_plane.clone();
        let event = event.clone();
        zeroclaw_spawn::spawn!(async move {
            project_with(Some(&control_plane), &event).await;
        });
    }
}

/// Project one event using the process-global control plane.
pub async fn project(event: &A2aRunEvent) {
    project_with(control_plane(), event).await;
}

/// Project one event onto an explicit control plane (testable). Registers a
/// `TaskKind::A2a` task on first sight and settles it once terminal.
pub async fn project_with(control_plane: Option<&ControlPlaneHandle>, event: &A2aRunEvent) {
    let Some(control_plane) = control_plane else {
        return;
    };
    let task_id = a2a_task_id(&event.peer, &event.remote_task_id);

    let existing = control_plane.store.get(&task_id).await.ok().flatten();
    if existing.is_none() {
        let _ = producer::register_producer_task(
            control_plane.store.as_ref(),
            &control_plane.boot_id,
            TaskKind::A2a,
            "a2a",
            &task_id,
            None,
        )
        .await;
    }

    if event.state.is_terminal()
        && let Ok(Some(record)) = control_plane.store.get(&task_id).await
        && !record.status.is_terminal()
    {
        let status = match &event.state {
            TaskState::TaskStateCompleted => TaskStatus::Completed,
            TaskState::TaskStateCanceled => TaskStatus::Cancelled,
            _ => TaskStatus::Failed,
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn event(peer: &str, id: &str, state: TaskState) -> A2aRunEvent {
        A2aRunEvent {
            peer: peer.into(),
            remote_task_id: id.into(),
            state,
        }
    }

    #[tokio::test]
    async fn a2a_run_registers_then_settles_through_the_canonical_store() {
        let dir = tempfile::tempdir().unwrap();
        let cp = ControlPlaneHandle::open(dir.path()).unwrap();

        // No control plane -> no-op.
        project_with(None, &event("p", "t", TaskState::TaskStateWorking)).await;

        project_with(
            Some(&cp),
            &event("peer-1", "run-1", TaskState::TaskStateSubmitted),
        )
        .await;
        let task_id = a2a_task_id("peer-1", "run-1");
        let rec = cp.store.get(&task_id).await.unwrap().unwrap();
        assert_eq!(rec.kind, TaskKind::A2a);
        assert_eq!(rec.status, TaskStatus::Queued);

        project_with(
            Some(&cp),
            &event("peer-1", "run-1", TaskState::TaskStateCompleted),
        )
        .await;
        assert_eq!(
            cp.store.get(&task_id).await.unwrap().unwrap().status,
            TaskStatus::Completed
        );
    }

    #[tokio::test]
    async fn terminal_only_event_registers_then_settles_failed() {
        let dir = tempfile::tempdir().unwrap();
        let cp = ControlPlaneHandle::open(dir.path()).unwrap();
        project_with(Some(&cp), &event("p", "r", TaskState::TaskStateFailed)).await;
        assert_eq!(
            cp.store
                .get(&a2a_task_id("p", "r"))
                .await
                .unwrap()
                .unwrap()
                .status,
            TaskStatus::Failed
        );
    }
}