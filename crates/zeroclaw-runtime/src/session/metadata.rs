//! Bounded, process-wide session metadata projections (P1.5).
//!
//! Complements the durable session store: this registry holds short-lived,
//! observable session facts a projection may read without a schema migration.
//! It is intentionally bounded in both the number of sessions and the entries
//! per session, so a long-running daemon cannot grow it without limit.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};

/// Max distinct sessions retained.
const MAX_SESSIONS: usize = 1_000;
/// Max retained provider-fallback entries per session (newest kept).
const MAX_FALLBACKS_PER_SESSION: usize = 20;

/// One recorded provider fallback for a session.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SessionProviderFallback {
    pub requested_provider: String,
    pub actual_provider: String,
    pub requested_model: String,
    pub actual_model: String,
    pub at: String,
}

/// The observable metadata projection for one session.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SessionMetadata {
    pub provider_fallbacks: Vec<SessionProviderFallback>,
    /// Number of context compactions (history trims) applied to this session.
    pub compactions: u64,
}

static REGISTRY: OnceLock<Mutex<HashMap<String, VecDeque<SessionProviderFallback>>>> = OnceLock::new();
static COMPACTIONS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

fn compaction_registry() -> &'static Mutex<HashMap<String, u64>> {
    COMPACTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Record one context compaction (history trim) applied to `session_key`.
pub fn record_compaction(session_key: &str) {
    if session_key.trim().is_empty() {
        return;
    }
    let Ok(mut map) = compaction_registry().lock() else {
        return;
    };
    if !map.contains_key(session_key) && map.len() >= MAX_SESSIONS
        && let Some(key) = map.keys().next().cloned()
    {
        map.remove(&key);
    }
    *map.entry(session_key.to_string()).or_insert(0) += 1;
}

/// Number of recorded compactions for `session_key`.
pub fn compaction_count(session_key: &str) -> u64 {
    compaction_registry()
        .lock()
        .ok()
        .and_then(|map| map.get(session_key).copied())
        .unwrap_or(0)
}

fn registry() -> &'static Mutex<HashMap<String, VecDeque<SessionProviderFallback>>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Record a provider fallback observed for `session_key`.
pub fn record_provider_fallback(session_key: &str, fallback: SessionProviderFallback) {
    if session_key.trim().is_empty() {
        return;
    }
    let Ok(mut map) = registry().lock() else {
        return;
    };
    if !map.contains_key(session_key) && map.len() >= MAX_SESSIONS {
        // Drop an arbitrary oldest-inserted session to stay bounded. Sessions
        // are keyed by an opaque id, so eviction order is best-effort.
        if let Some(key) = map.keys().next().cloned() {
            map.remove(&key);
        }
    }
    let entry = map.entry(session_key.to_string()).or_default();
    if entry.len() == MAX_FALLBACKS_PER_SESSION {
        entry.pop_front();
    }
    entry.push_back(fallback);
}

/// Read the metadata projection for `session_key` (empty when none recorded).
pub fn session_metadata(session_key: &str) -> SessionMetadata {
    let provider_fallbacks = registry()
        .lock()
        .ok()
        .and_then(|map| {
            map.get(session_key)
                .map(|entries| entries.iter().cloned().collect())
        })
        .unwrap_or_default();
    SessionMetadata {
        provider_fallbacks,
        compactions: compaction_count(session_key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fallback(provider: &str) -> SessionProviderFallback {
        SessionProviderFallback {
            requested_provider: provider.into(),
            actual_provider: format!("{provider}-fallback"),
            requested_model: "m1".into(),
            actual_model: "m2".into(),
            at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn records_and_reads_fallbacks_per_session() {
        let key = format!("session-{}", uuid::Uuid::new_v4());
        assert!(session_metadata(&key).provider_fallbacks.is_empty());
        record_provider_fallback(&key, fallback("a"));
        record_provider_fallback(&key, fallback("b"));
        let meta = session_metadata(&key);
        assert_eq!(meta.provider_fallbacks.len(), 2);
        assert_eq!(meta.provider_fallbacks[0].requested_provider, "a");
    }

    #[test]
    fn empty_key_is_ignored_and_entries_are_bounded() {
        let key = format!("session-{}", uuid::Uuid::new_v4());
        record_provider_fallback("", fallback("ignored"));
        for i in 0..(MAX_FALLBACKS_PER_SESSION + 5) {
            record_provider_fallback(&key, fallback(&format!("p{i}")));
        }
        assert_eq!(
            session_metadata(&key).provider_fallbacks.len(),
            MAX_FALLBACKS_PER_SESSION
        );
    }

    #[test]
    fn records_compactions_per_session() {
        let key = format!("session-{}", uuid::Uuid::new_v4());
        assert_eq!(compaction_count(&key), 0);
        record_compaction(&key);
        record_compaction(&key);
        assert_eq!(compaction_count(&key), 2);
        assert_eq!(session_metadata(&key).compactions, 2);
        record_compaction("");
        assert_eq!(compaction_count(""), 0);
    }
}