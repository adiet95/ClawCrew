//! Session lifecycle contract.
//!
//! Provides the canonical state machine and unified lifecycle checks for sessions.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Session is active and accepting work.
    Active,
    /// Session is paused and will reject new work until resumed.
    Paused,
    /// Session is idle and eligible for compaction or expiry.
    Idle,
    /// Session is being gracefully torn down.
    Closing,
    /// Session is closed and resources are released.
    Closed,
}

/// Classification of a session by its owning surface (P0.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionType {
    /// Interactive user/agent session.
    User,
    /// Cron-triggered session.
    Cron,
    /// Subagent/child session.
    Subagent,
    /// TaskRunner workflow session.
    Workflow,
}

impl Default for SessionType {
    fn default() -> Self {
        Self::User
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionLifecycle {
    pub state: SessionState,
    pub created_at: String,
    pub last_active_at: String,
    pub provider_healthy: bool,
    /// Owning agent/user alias. Persisted so a stale process cannot operate on a
    /// successor session's state.
    #[serde(default)]
    pub owner: String,
    /// Monotonic generation guard; resets and removes bump it (P0.3).
    #[serde(default)]
    pub generation: u64,
    /// Session type, persisted for admission/visibility checks.
    #[serde(default)]
    pub session_type: SessionType,
    /// Workspace boundary the session is bound to, when any.
    #[serde(default)]
    pub workspace: Option<String>,
    /// Recovery decision assigned at restart: `resumed`, `fresh`, `lost`, or
    /// `needs_review` (P0.3).
    #[serde(default)]
    pub recovery_decision: Option<String>,
    /// RFC3339 timestamp after which an idle session is eligible for cleanup.
    #[serde(default)]
    pub idle_expires_at: Option<String>,
}

impl SessionLifecycle {
    pub fn new() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            state: SessionState::Active,
            created_at: now.clone(),
            last_active_at: now,
            provider_healthy: true,
            owner: String::new(),
            generation: 0,
            session_type: SessionType::User,
            workspace: None,
            recovery_decision: None,
            idle_expires_at: None,
        }
    }

    pub fn mark_active(&mut self) {
        self.state = SessionState::Active;
        self.last_active_at = chrono::Utc::now().to_rfc3339();
        self.idle_expires_at = None;
    }

    pub fn mark_idle(&mut self) {
        if self.state == SessionState::Active {
            self.state = SessionState::Idle;
        }
    }

    /// Transition an active/idle session to [`SessionState::Idle`] and arm an
    /// idle-expiry deadline so orphaned sessions fail closed rather than linger.
    pub fn arm_idle_expiry(&mut self, timeout: chrono::Duration) {
        self.mark_idle();
        if self.state == SessionState::Idle {
            self.idle_expires_at = Some((chrono::Utc::now() + timeout).to_rfc3339());
        }
    }

    /// True when an armed idle deadline has passed and the session is not closed.
    pub fn is_expired(&self) -> bool {
        if self.state == SessionState::Closed {
            return false;
        }
        if let Some(ref expires) = self.idle_expires_at {
            if let Ok(t) = chrono::DateTime::parse_from_rfc3339(expires) {
                return chrono::Utc::now() >= t.with_timezone(&chrono::Utc);
            }
        }
        false
    }

    /// Fail-closed orphan cleanup: close the session if its idle deadline passed.
    /// Returns `true` when the session was closed.
    pub fn expire_if_idle(&mut self) -> bool {
        if self.is_expired() {
            self.close();
            true
        } else {
            false
        }
    }

    pub fn close(&mut self) {
        self.state = SessionState::Closed;
    }

    pub fn is_accepting_work(&self) -> bool {
        matches!(self.state, SessionState::Active | SessionState::Idle)
    }

    pub fn report_provider_error(&mut self) {
        self.provider_healthy = false;
    }

    pub fn report_provider_success(&mut self) {
        self.provider_healthy = true;
    }
}

impl Default for SessionLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session_defaults_are_consistent() {
        let s = SessionLifecycle::new();
        assert_eq!(s.state, SessionState::Active);
        assert!(s.provider_healthy);
        assert_eq!(s.owner, "");
        assert_eq!(s.generation, 0);
        assert_eq!(s.session_type, SessionType::User);
        assert!(s.workspace.is_none());
        assert!(s.recovery_decision.is_none());
        assert!(s.idle_expires_at.is_none());
        assert!(s.is_accepting_work());
    }

    #[test]
    fn idle_expiry_closes_after_deadline_and_not_before() {
        let mut s = SessionLifecycle::new();
        s.arm_idle_expiry(chrono::Duration::milliseconds(40));
        assert_eq!(s.state, SessionState::Idle);
        assert!(!s.is_expired());
        assert!(!s.expire_if_idle());

        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(s.is_expired());
        assert!(s.expire_if_idle());
        assert_eq!(s.state, SessionState::Closed);
        // A closed session is never "expired" again.
        assert!(!s.is_expired());
    }

    #[test]
    fn mark_active_clears_idle_deadline() {
        let mut s = SessionLifecycle::new();
        s.arm_idle_expiry(chrono::Duration::minutes(5));
        assert!(s.idle_expires_at.is_some());
        s.mark_active();
        assert_eq!(s.state, SessionState::Active);
        assert!(s.idle_expires_at.is_none());
    }

    #[test]
    fn legacy_payload_deserializes_with_new_field_defaults() {
        let legacy = r#"{
            "state": "active",
            "created_at": "2026-01-01T00:00:00Z",
            "last_active_at": "2026-01-01T00:00:00Z",
            "provider_healthy": true
        }"#;
        let s: SessionLifecycle = serde_json::from_str(legacy).unwrap();
        assert_eq!(s.owner, "");
        assert_eq!(s.generation, 0);
        assert_eq!(s.session_type, SessionType::User);
    }
}