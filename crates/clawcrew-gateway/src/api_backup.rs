//! Gateway HTTP endpoints for the backup/restore/schema-versions surface
//! described in `docs/release-and-recovery.md`.
//!
//! - `GET  /api/backup/schema-versions` — operator-readable durable schema map.
//! - `POST /api/backup/create`          — snapshot the known durable stores.
//! - `POST /api/backup/plan-restore`    — dry-run a restore (verify + actions).
//! - `POST /api/backup/restore`         — apply a verified backup (journaled).

use super::api::require_auth;
use super::AppState;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};

use clawcrew_runtime::platform::backup;

// ── Schema versions ──────────────────────────────────────────────────────────

/// Response body for `GET /api/backup/schema-versions`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
pub struct SchemaVersionsResponse {
    pub versions: Vec<SchemaVersionEntry>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
pub struct SchemaVersionEntry {
    pub store: String,
    pub version: u64,
}

/// `GET /api/backup/schema-versions`
///
/// Returns the canonical, operator-readable list of durable schema versions
/// this build understands. Mirrors `backup::schema_versions()`.
pub async fn handle_schema_versions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }
    let versions = backup::schema_versions()
        .into_iter()
        .map(|(store, version)| SchemaVersionEntry {
            store: store.to_string(),
            version,
        })
        .collect();
    Json(SchemaVersionsResponse { versions }).into_response()
}

// ── Backup create ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
pub struct CreateBackupRequest {
    /// Destination directory for the backup. Relative paths are resolved
    /// against the data directory.
    pub dest: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
pub struct CreateBackupResponse {
    pub entries: usize,
    pub manifest_path: String,
}

/// `POST /api/backup/create`
///
/// Snapshot the known durable stores into `dest` and write `manifest.json`.
pub async fn handle_create_backup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateBackupRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }
    let data_dir = state.config.read().data_dir.clone();
    let dest = if std::path::Path::new(&req.dest).is_absolute() {
        std::path::PathBuf::from(&req.dest)
    } else {
        data_dir.join(&req.dest)
    };
    match backup::create_backup(&data_dir, &dest, backup::default_store_paths()) {
        Ok(manifest) => Json(CreateBackupResponse {
            entries: manifest.entries.len(),
            manifest_path: dest.join("manifest.json").to_string_lossy().into_owned(),
        })
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("{e:#}") })),
        )
            .into_response(),
    }
}

// ── Plan restore (dry run) ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
pub struct RestoreRequest {
    /// Path to the backup directory containing `manifest.json`.
    pub backup_dir: String,
}

/// `POST /api/backup/plan-restore`
///
/// Dry-run: verify every backup file's digest and report the actions a restore
/// would take. Touches nothing.
pub async fn handle_plan_restore(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RestoreRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }
    let data_dir = state.config.read().data_dir.clone();
    let backup_dir = std::path::PathBuf::from(&req.backup_dir);
    match backup::plan_restore(&backup_dir, &data_dir) {
        Ok(plan) => Json(plan).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": format!("{e:#}") })),
        )
            .into_response(),
    }
}

// ── Restore (journaled) ─────────────────────────────────────────────────────

/// `POST /api/backup/restore`
///
/// Apply a verified backup using journaled all-or-nothing swap. The caller
/// should have previously called `plan-restore` to confirm compatibility.
pub async fn handle_restore(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RestoreRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }
    let data_dir = state.config.read().data_dir.clone();
    let backup_dir = std::path::PathBuf::from(&req.backup_dir);
    match backup::restore_backup_journaled(&backup_dir, &data_dir) {
        Ok(count) => Json(serde_json::json!({
            "restored": count,
            "journaled": true,
        }))
        .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": format!("{e:#}") })),
        )
            .into_response(),
    }
}
