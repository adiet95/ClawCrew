//! Tamper-evident security audit feed projection.
//!
//! Reads the bounded tail of the audit log and reports the Merkle-chain
//! verification result. Payloads are redacted before egress so credentials,
//! prompts, and sensitive tool arguments never reach the dashboard.

use super::{AppState, api::require_auth};
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Json, Response},
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};

const MAX_AUDIT_PAGE: usize = 200;

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    pub limit: Option<usize>,
}

fn bounds(query: &AuditQuery) -> usize {
    query.limit.unwrap_or(100).clamp(1, MAX_AUDIT_PAGE)
}

/// Read the last `limit` non-empty lines of the audit log without loading the
/// whole file, using a ring buffer.
fn tail_lines(path: &std::path::Path, limit: usize) -> std::io::Result<Vec<String>> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut ring: std::collections::VecDeque<String> = std::collections::VecDeque::with_capacity(limit);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if ring.len() == limit {
            ring.pop_front();
        }
        ring.push_back(line);
    }
    Ok(ring.into_iter().collect())
}

fn redact_value(value: Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| {
                    let sensitive = [
                        "token",
                        "secret",
                        "password",
                        "api_key",
                        "authorization",
                        "credential",
                        "prompt",
                        "argument",
                        "args",
                    ]
                    .iter()
                    .any(|part| key.to_ascii_lowercase().contains(part));
                    (
                        key,
                        if sensitive {
                            Value::String("[REDACTED]".into())
                        } else {
                            redact_value(value)
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(redact_value).collect()),
        other => other,
    }
}

/// GET /api/audit — bounded, redaction-aware view of the tamper-evident audit log.
pub async fn handle_api_audit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuditQuery>,
) -> Response {
    if let Err(response) = require_auth(&state, &headers) {
        return response.into_response();
    }
    let config = state.config.read().clone();
    let limit = bounds(&query);

    if !config.security.audit.enabled {
        return Json(serde_json::json!({
            "enabled": false,
            "verified": true,
            "entries": [],
        }))
        .into_response();
    }

    let log_path = config.data_dir.join(&config.security.audit.log_path);
    if !log_path.exists() {
        return Json(serde_json::json!({
            "enabled": true,
            "verified": true,
            "entries": [],
        }))
        .into_response();
    }

    let lines = match tail_lines(&log_path, limit) {
        Ok(lines) => lines,
        Err(error) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("failed to read audit log: {error}") })),
            )
                .into_response();
        }
    };

    // Newest first for the feed.
    let mut entries: Vec<Value> = lines
        .into_iter()
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .map(redact_value)
        .collect();
    entries.reverse();

    let verified = clawcrew_runtime::security::audit::verify_chain(&log_path).is_ok();

    Json(serde_json::json!({
        "enabled": true,
        "verified": verified,
        "entries": entries,
    }))
    .into_response()
}

/// Pure helpers for unit tests.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_clamps_and_defaults() {
        assert_eq!(bounds(&AuditQuery { limit: None }), 100);
        assert_eq!(bounds(&AuditQuery { limit: Some(0) }), 1);
        assert_eq!(bounds(&AuditQuery { limit: Some(999) }), MAX_AUDIT_PAGE);
    }

    #[test]
    fn redact_masks_sensitive_keys_recursively() {
        let value = serde_json::json!({
            "event_type": "command_execution",
            "action": { "command": "ls", "api_key": "sk-live" },
            "nested": [{ "token": "abc", "ok": true }],
        });
        let redacted = redact_value(value);
        assert_eq!(redacted["action"]["api_key"], "[REDACTED]");
        assert_eq!(redacted["action"]["command"], "ls");
        assert_eq!(redacted["nested"][0]["token"], "[REDACTED]");
        assert_eq!(redacted["nested"][0]["ok"], true);
    }

    #[test]
    fn tail_lines_returns_last_n_in_order() {
        let path = std::env::temp_dir().join(format!(
            "zc-audit-tail-{}-{}.log",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, "a\nb\nc\nd\n").unwrap();
        assert_eq!(tail_lines(&path, 2).unwrap(), vec!["c", "d"]);
        assert_eq!(tail_lines(&path, 10).unwrap().len(), 4);
        let _ = std::fs::remove_file(&path);
    }
}
