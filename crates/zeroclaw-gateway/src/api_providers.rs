//! Provider health projection.
//!
//! Projects each configured agent's resolved provider/model joined with the
//! process health registry. A provider with no recorded probe reports
//! `"unknown"` rather than a fabricated healthy status.

use super::{AppState, api::require_auth};
use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;
use std::collections::HashMap;

use zeroclaw_runtime::agent::reliability::{ProviderHealthRecord, provider_snapshot};

#[derive(Debug, Serialize)]
struct ProviderHealthRow {
    agent: String,
    provider: Option<String>,
    model: String,
    status: String,
    last_ok: Option<String>,
    last_error: Option<String>,
    restart_count: u64,
    /// Observed mean latency (ms) from the live registry, when recorded.
    avg_latency_ms: Option<f64>,
}

fn status_from_health(
    health: &zeroclaw_runtime::health::HealthSnapshot,
    provider: Option<&str>,
) -> (String, Option<String>, Option<String>, u64) {
    let entry = provider
        .map(|p| format!("provider:{p}"))
        .and_then(|component| health.components.get(&component));
    match entry {
        Some(e) => (
            e.status.clone(),
            e.last_ok.clone(),
            e.last_error.clone(),
            e.restart_count,
        ),
        None => ("unknown".to_string(), None, None, 0),
    }
}

/// GET /api/providers/health — per-agent provider health projection.
pub async fn handle_providers_health(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = require_auth(&state, &headers) {
        return response.into_response();
    }
    let config = state.config.read().clone();
    let health = zeroclaw_runtime::health::snapshot();
    let live: HashMap<String, ProviderHealthRecord> = provider_snapshot()
        .into_iter()
        .map(|record| (record.provider_name.clone(), record))
        .collect();

    let mut providers: Vec<ProviderHealthRow> = Vec::new();
    for alias in config.agents.keys() {
        let resolved = config.resolved_model_provider_for_agent(alias);
        let provider = resolved.map(|(ty, entry_alias, _)| format!("{ty}.{entry_alias}"));
        let model = resolved
            .and_then(|(_, _, entry)| entry.model.clone())
            .unwrap_or_default();
        // Prefer the live registry (fed at the provider-call boundary) over the
        // generic component health, which providers do not register into.
        let (status, last_ok, last_error, restart_count, avg_latency_ms) =
            match provider.as_deref().and_then(|p| live.get(p)) {
                Some(record) => {
                    let status = if record.is_in_cooldown() || record.consecutive_failures > 0 {
                        "error"
                    } else if record.total_invocations > 0 {
                        "ok"
                    } else {
                        "unknown"
                    };
                    (status.to_string(), None, None, 0, record.avg_latency_ms())
                }
                None => {
                    let (status, last_ok, last_error, restart_count) =
                        status_from_health(&health, provider.as_deref());
                    (status, last_ok, last_error, restart_count, None)
                }
            };
        providers.push(ProviderHealthRow {
            agent: alias.clone(),
            provider,
            model,
            status,
            last_ok,
            last_error,
            restart_count,
            avg_latency_ms,
        });
    }

    Json(serde_json::json!({ "providers": providers })).into_response()
}

/// One row of the provider capability documentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderDocEntry {
    pub name: String,
    pub display_name: String,
    pub local: bool,
}

/// Render Markdown provider capability documentation from the canonical
/// provider catalog (P3.3).
pub fn render_provider_capability_docs(entries: &[ProviderDocEntry]) -> String {
    let mut out = String::from("# Provider Capabilities\n\n");
    out.push_str(
        "Generated from the canonical provider catalog (`zeroclaw_providers::list_model_providers`).\n\
         Capability flags (vision/tools) are declared per model, not per provider.\n\n",
    );
    out.push_str("| Provider | Display name | Network |\n|---|---|---|\n");
    for entry in entries {
        out.push_str(&format!(
            "| `{}` | {} | {} |\n",
            entry.name,
            entry.display_name,
            if entry.local { "local" } else { "remote" }
        ));
    }
    out
}

/// GET /api/providers/capabilities.md — provider capability documentation.
pub async fn handle_provider_capabilities(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = require_auth(&state, &headers) {
        return response.into_response();
    }
    let entries: Vec<ProviderDocEntry> = zeroclaw_providers::list_model_providers()
        .into_iter()
        .map(|p| ProviderDocEntry {
            name: p.name.to_string(),
            display_name: p.display_name.to_string(),
            local: p.local,
        })
        .collect();
    (
        [(axum::http::header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        render_provider_capability_docs(&entries),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_docs_render_a_row_per_provider() {
        let docs = render_provider_capability_docs(&[
            ProviderDocEntry {
                name: "openai".into(),
                display_name: "OpenAI".into(),
                local: false,
            },
            ProviderDocEntry {
                name: "ollama".into(),
                display_name: "Ollama".into(),
                local: true,
            },
        ]);
        assert!(docs.starts_with("# Provider Capabilities"));
        assert!(docs.contains("| `openai` | OpenAI | remote |"));
        assert!(docs.contains("| `ollama` | Ollama | local |"));
    }
}
