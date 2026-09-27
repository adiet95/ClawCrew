//! Outbound A2A run observer seam.
//!
//! `clawcrew-tools` dispatches A2A runs but sits below the runtime control
//! plane. This tiny, dependency-free seam lets the tools emit run-state events
//! that the runtime observes and projects onto the canonical `TaskRegistry`,
//! without a backwards crate dependency.
//!
//! Emission is best-effort and synchronous: when no observer is installed it is
//! a no-op, so the tools behave exactly as before in one-shot/CLI contexts.

use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};

use crate::a2a_wire::TaskState;

/// One observed transition of an outbound A2A run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aRunEvent {
    /// Configured peer name the run was sent to.
    pub peer: String,
    /// The peer's task id.
    pub remote_task_id: String,
    /// The peer-reported task state.
    pub state: TaskState,
}

/// Receives outbound A2A run events. Implementors must not block.
pub trait A2aRunObserver: Send + Sync {
    fn on_run_event(&self, event: &A2aRunEvent);
}

static OBSERVER: OnceLock<Arc<dyn A2aRunObserver>> = OnceLock::new();

/// Install the process-wide observer. Returns `false` when one is already set
/// (install once at daemon boot).
pub fn set_a2a_run_observer(observer: Arc<dyn A2aRunObserver>) -> bool {
    OBSERVER.set(observer).is_ok()
}

/// Emit a run event to the installed observer (no-op when none is installed).
pub fn emit_a2a_run_event(event: &A2aRunEvent) {
    if let Some(observer) = OBSERVER.get() {
        observer.on_run_event(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct RecordingObserver(Mutex<Vec<A2aRunEvent>>);
    impl A2aRunObserver for RecordingObserver {
        fn on_run_event(&self, event: &A2aRunEvent) {
            self.0.lock().unwrap().push(event.clone());
        }
    }

    #[test]
    fn emit_is_a_noop_without_an_observer() {
        // Must not panic when nothing is installed.
        emit_a2a_run_event(&A2aRunEvent {
            peer: "p".into(),
            remote_task_id: "t".into(),
            state: TaskState::TaskStateWorking,
        });
    }

    #[test]
    fn task_state_terminal_classification() {
        assert!(TaskState::TaskStateCompleted.is_terminal());
        assert!(TaskState::TaskStateFailed.is_terminal());
        assert!(TaskState::TaskStateCanceled.is_terminal());
        assert!(TaskState::TaskStateRejected.is_terminal());
        assert!(!TaskState::TaskStateWorking.is_terminal());
        assert!(!TaskState::TaskStateSubmitted.is_terminal());
        assert!(!TaskState::TaskStateInputRequired.is_terminal());
    }

    #[test]
    fn observer_receives_events_once_installed() {
        let recorder = Arc::new(RecordingObserver(Mutex::new(Vec::new())));
        // Other tests may have installed first; only assert when we win.
        let installed = set_a2a_run_observer(recorder.clone());
        let event = A2aRunEvent {
            peer: "peer-1".into(),
            remote_task_id: "task-1".into(),
            state: TaskState::TaskStateSubmitted,
        };
        emit_a2a_run_event(&event);
        if installed {
            assert_eq!(recorder.0.lock().unwrap().as_slice(), std::slice::from_ref(&event));
        }
    }
}