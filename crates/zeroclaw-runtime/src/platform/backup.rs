//! Durable-store backup manifest and integrity checks (P3.2).
//!
//! A backup is a set of named store files plus a versioned manifest carrying
//! each file's size and SHA-256 digest, so a restore can refuse a tampered,
//! truncated, or partial backup before touching live state.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Version of the backup manifest contract. A higher on-disk version is refused.
pub const BACKUP_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupEntry {
    /// Stable store name (e.g. `control_plane`).
    pub name: String,
    /// Path relative to the backup root.
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    #[serde(default)]
    pub schema_version: u32,
    pub created_at: String,
    #[serde(default)]
    pub entries: Vec<BackupEntry>,
}

/// Lowercase hex SHA-256 of a file's bytes.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex::encode(hasher.finalize()))
}

/// Known durable stores, as `(name, path-relative-to-data-dir)`.
pub fn default_store_paths() -> &'static [(&'static str, &'static str)] {
    &[
        ("control_plane", "control_plane.db"),
        ("app_registry", "apps/registry.json"),
    ]
}

/// The durable schema versions this runtime understands, as a projection for
/// operators so an upgrade/restore can be checked before it is applied.
pub fn schema_versions() -> Vec<(&'static str, u64)> {
    vec![
        (
            "control_plane",
            crate::control_plane::task_store_sqlite::CONTROL_PLANE_SCHEMA_VERSION as u64,
        ),
        (
            "task_status_contract",
            crate::control_plane::task_registry::TASK_STATUS_CONTRACT_VERSION as u64,
        ),
        (
            "task_runner",
            crate::control_plane::task_runner::TASK_RUNNER_SCHEMA_VERSION as u64,
        ),
        (
            "app_registry",
            crate::platform::app_registry::APP_REGISTRY_SCHEMA_VERSION as u64,
        ),
        (
            "config",
            zeroclaw_config::migration::CURRENT_SCHEMA_VERSION as u64,
        ),
        (
            "memory_sqlite",
            zeroclaw_config::schema::v2::SQLITE_MEMORY_SCHEMA_VERSION.max(0) as u64,
        ),
        (
            "session_sqlite",
            zeroclaw_infra::session_sqlite::SESSION_SQLITE_SCHEMA_VERSION,
        ),
        ("backup_manifest", BACKUP_MANIFEST_SCHEMA_VERSION as u64),
    ]
}

/// What a restore would do to one store file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreAction {
    Create { name: String, path: String },
    Overwrite { name: String, path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorePlan {
    pub entries: usize,
    pub compatible: bool,
    pub actions: Vec<RestoreAction>,
    pub notes: Vec<String>,
}

/// Copy the existing store files under `root` into `dest` and write
/// `manifest.json`. Absent stores are skipped.
pub fn create_backup(
    root: &Path,
    dest: &Path,
    stores: &[(&str, &str)],
) -> anyhow::Result<BackupManifest> {
    std::fs::create_dir_all(dest)?;
    let manifest = build_manifest(root, stores)?;
    for entry in &manifest.entries {
        let src = root.join(&entry.path);
        let dst = dest.join(&entry.path);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&src, &dst)?;
    }
    std::fs::write(
        dest.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(manifest)
}

/// Load a backup manifest from a backup directory.
pub fn load_manifest(backup_dir: &Path) -> anyhow::Result<BackupManifest> {
    let raw = std::fs::read_to_string(backup_dir.join("manifest.json"))?;
    Ok(serde_json::from_str(&raw)?)
}

/// Dry-run: report the actions a restore would take and whether the manifest is
/// compatible, verifying every backup file's digest first. Touches nothing.
pub fn plan_restore(backup_dir: &Path, target_root: &Path) -> anyhow::Result<RestorePlan> {
    let manifest = load_manifest(backup_dir)?;
    let compatible = manifest.schema_version <= BACKUP_MANIFEST_SCHEMA_VERSION;
    let mut notes = Vec::new();
    if !compatible {
        notes.push(format!(
            "manifest schema {} is newer than supported {}",
            manifest.schema_version, BACKUP_MANIFEST_SCHEMA_VERSION
        ));
    }
    let mut actions = Vec::new();
    for entry in &manifest.entries {
        let src = backup_dir.join(&entry.path);
        anyhow::ensure!(
            src.is_file(),
            "backup file for {} is missing at {}",
            entry.name,
            src.display()
        );
        let sha = sha256_file(&src)?;
        anyhow::ensure!(
            sha.eq_ignore_ascii_case(&entry.sha256),
            "backup file for {} failed its digest check",
            entry.name
        );
        let path = entry.path.clone();
        let action = if target_root.join(&entry.path).exists() {
            RestoreAction::Overwrite {
                name: entry.name.clone(),
                path,
            }
        } else {
            RestoreAction::Create {
                name: entry.name.clone(),
                path,
            }
        };
        actions.push(action);
    }
    Ok(RestorePlan {
        entries: actions.len(),
        compatible,
        actions,
        notes,
    })
}

/// Restore a verified backup into `target_root`. Each file is written via a
/// temporary then atomically renamed, so a crash mid-restore never leaves a
/// half-written store. Refuses an incompatible manifest. Returns the number of
/// files restored. (A "rollback" is restoring a prior backup the same way.)
pub fn restore_backup(backup_dir: &Path, target_root: &Path) -> anyhow::Result<usize> {
    let plan = plan_restore(backup_dir, target_root)?;
    anyhow::ensure!(
        plan.compatible,
        "backup manifest is incompatible with this runtime"
    );
    let manifest = load_manifest(backup_dir)?;
    for entry in &manifest.entries {
        let src = backup_dir.join(&entry.path);
        let dst = target_root.join(&entry.path);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = dst.with_extension("restore.tmp");
        std::fs::copy(&src, &tmp)?;
        std::fs::rename(&tmp, &dst)?;
    }
    Ok(plan.entries)
}

/// Journal file written at the restore root while a cross-store swap is in
/// flight, so a crash can be finished on next startup.
pub const RESTORE_JOURNAL: &str = "restore.journal";

/// Restore a verified backup as an **all-or-nothing cross-store swap**:
/// every file is staged first (no live store touched), a journal is written,
/// then the staged files are swapped into place and the journal removed. A
/// crash between staging and swap is finished by [`recover_pending_restore`].
pub fn restore_backup_journaled(
    backup_dir: &Path,
    target_root: &Path,
) -> anyhow::Result<usize> {
    let plan = plan_restore(backup_dir, target_root)?;
    anyhow::ensure!(
        plan.compatible,
        "backup manifest is incompatible with this runtime"
    );
    let manifest = load_manifest(backup_dir)?;

    // Phase 1: stage every file; live stores are untouched.
    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new();
    for entry in &manifest.entries {
        let src = backup_dir.join(&entry.path);
        let final_path = target_root.join(&entry.path);
        if let Some(parent) = final_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let stage = final_path.with_extension("restore.pending");
        std::fs::copy(&src, &stage)?;
        staged.push((stage, final_path));
    }

    // Phase 2: journal the swap, then apply it.
    let journal = target_root.join(RESTORE_JOURNAL);
    let journal_entries: Vec<serde_json::Value> = staged
        .iter()
        .map(|(stage, final_path)| {
            serde_json::json!({
                "stage": stage.to_string_lossy(),
                "final": final_path.to_string_lossy(),
            })
        })
        .collect();
    std::fs::write(&journal, serde_json::to_string(&journal_entries)?)?;
    apply_restore_journal(&journal)?;
    Ok(manifest.entries.len())
}

/// Finish a journaled restore that crashed before completing its swap.
/// Idempotent: a missing journal is a no-op. Returns `1` when a journal was
/// applied.
pub fn recover_pending_restore(target_root: &Path) -> anyhow::Result<usize> {
    let journal = target_root.join(RESTORE_JOURNAL);
    if !journal.is_file() {
        return Ok(0);
    }
    apply_restore_journal(&journal)?;
    Ok(1)
}

fn apply_restore_journal(journal: &Path) -> anyhow::Result<()> {
    let raw = std::fs::read_to_string(journal)?;
    let entries: Vec<serde_json::Value> = serde_json::from_str(&raw)?;
    for entry in &entries {
        let stage = entry
            .get("stage")
            .and_then(serde_json::Value::as_str)
            .map(PathBuf::from);
        let final_path = entry
            .get("final")
            .and_then(serde_json::Value::as_str)
            .map(PathBuf::from);
        if let (Some(stage), Some(final_path)) = (stage, final_path)
            && stage.is_file()
        {
            // `rename` replaces atomically on the same filesystem.
            std::fs::rename(&stage, &final_path)?;
        }
    }
    std::fs::remove_file(journal)?;
    Ok(())
}

/// Build a manifest over the named store files that exist under `root`. Absent
/// stores are skipped (a fresh install legitimately lacks some), so the manifest
/// describes exactly what a restore would bring back.
pub fn build_manifest(
    root: &Path,
    stores: &[(&str, &str)],
) -> anyhow::Result<BackupManifest> {
    let mut entries = Vec::new();
    for (name, relative) in stores {
        let path = root.join(relative);
        if !path.is_file() {
            continue;
        }
        let size_bytes = std::fs::metadata(&path)?.len();
        let sha256 = sha256_file(&path)?;
        entries.push(BackupEntry {
            name: (*name).to_string(),
            path: (*relative).to_string(),
            size_bytes,
            sha256,
        });
    }
    Ok(BackupManifest {
        schema_version: BACKUP_MANIFEST_SCHEMA_VERSION,
        created_at: chrono::Utc::now().to_rfc3339(),
        entries,
    })
}

/// Verify a manifest against the files under `root`. Refuses a newer schema and
/// fails closed on a missing file, a size mismatch, or a digest mismatch.
/// Returns the number of verified entries.
pub fn verify_manifest(root: &Path, manifest: &BackupManifest) -> anyhow::Result<usize> {
    anyhow::ensure!(
        manifest.schema_version <= BACKUP_MANIFEST_SCHEMA_VERSION,
        "backup manifest schema {} is newer than supported {}",
        manifest.schema_version,
        BACKUP_MANIFEST_SCHEMA_VERSION
    );
    for entry in &manifest.entries {
        let path = root.join(&entry.path);
        anyhow::ensure!(
            path.is_file(),
            "backup entry {} is missing at {}",
            entry.name,
            path.display()
        );
        let size = std::fs::metadata(&path)?.len();
        anyhow::ensure!(
            size == entry.size_bytes,
            "backup entry {} size mismatch: on disk {size}, manifest {}",
            entry.name,
            entry.size_bytes
        );
        let sha = sha256_file(&path)?;
        anyhow::ensure!(
            sha.eq_ignore_ascii_case(&entry.sha256),
            "backup entry {} digest mismatch",
            entry.name
        );
    }
    Ok(manifest.entries.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "zc-backup-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn build_and_verify_roundtrip_skips_absent_stores() {
        let dir = temp_dir("roundtrip");
        std::fs::write(dir.join("control_plane.db"), b"durable-state").unwrap();
        // apps/registry.json absent → skipped.
        let manifest = build_manifest(&dir, default_store_paths()).unwrap();
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(manifest.entries[0].name, "control_plane");
        assert_eq!(
            manifest.entries[0].sha256,
            sha256_file(&dir.join("control_plane.db")).unwrap()
        );
        assert_eq!(verify_manifest(&dir, &manifest).unwrap(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_detects_tampering_size_drift_and_missing_files() {
        let dir = temp_dir("tamper");
        std::fs::write(dir.join("control_plane.db"), b"original").unwrap();
        let manifest = build_manifest(&dir, default_store_paths()).unwrap();

        // Digest mismatch.
        std::fs::write(dir.join("control_plane.db"), b"tampered!").unwrap();
        assert!(verify_manifest(&dir, &manifest).is_err());

        // Restore the exact bytes; now it verifies.
        std::fs::write(dir.join("control_plane.db"), b"original").unwrap();
        assert_eq!(verify_manifest(&dir, &manifest).unwrap(), 1);

        // Missing file.
        std::fs::remove_file(dir.join("control_plane.db")).unwrap();
        assert!(verify_manifest(&dir, &manifest).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_refuses_a_newer_schema() {
        let dir = temp_dir("schema");
        let mut manifest = build_manifest(&dir, default_store_paths()).unwrap();
        manifest.schema_version = BACKUP_MANIFEST_SCHEMA_VERSION + 1;
        assert!(verify_manifest(&dir, &manifest).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_restore_rollback_roundtrip_with_dry_run() {
        let src = temp_dir("src");
        let backup = temp_dir("bak");
        let target = temp_dir("restore");
        std::fs::write(src.join("control_plane.db"), b"state-v1").unwrap();
        create_backup(&src, &backup, default_store_paths()).unwrap();

        let plan = plan_restore(&backup, &target).unwrap();
        assert!(plan.compatible);
        assert_eq!(plan.entries, 1);
        assert!(matches!(plan.actions[0], RestoreAction::Create { .. }));

        assert_eq!(restore_backup(&backup, &target).unwrap(), 1);
        assert_eq!(
            std::fs::read(target.join("control_plane.db")).unwrap(),
            b"state-v1"
        );

        // Existing target -> dry-run reports Overwrite.
        let plan = plan_restore(&backup, &target).unwrap();
        assert!(matches!(plan.actions[0], RestoreAction::Overwrite { .. }));

        // Rollback = restore a prior backup over mutated state.
        std::fs::write(target.join("control_plane.db"), b"corrupted").unwrap();
        restore_backup(&backup, &target).unwrap();
        assert_eq!(
            std::fs::read(target.join("control_plane.db")).unwrap(),
            b"state-v1"
        );

        for dir in [src, backup, target] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn plan_restore_rejects_a_tampered_backup_file() {
        let src = temp_dir("src-tamper");
        let backup = temp_dir("bak-tamper");
        std::fs::write(src.join("control_plane.db"), b"good").unwrap();
        create_backup(&src, &backup, default_store_paths()).unwrap();
        std::fs::write(backup.join("control_plane.db"), b"evil").unwrap();
        assert!(plan_restore(&backup, &src).is_err());
        for dir in [src, backup] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn restore_refuses_an_incompatible_manifest() {
        let src = temp_dir("src-schema");
        let backup = temp_dir("bak-schema");
        std::fs::write(src.join("control_plane.db"), b"x").unwrap();
        create_backup(&src, &backup, default_store_paths()).unwrap();
        let mut manifest = load_manifest(&backup).unwrap();
        manifest.schema_version = BACKUP_MANIFEST_SCHEMA_VERSION + 1;
        std::fs::write(
            backup.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
        assert!(!plan_restore(&backup, &src).unwrap().compatible);
        assert!(restore_backup(&backup, &src).is_err());
        for dir in [src, backup] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn schema_versions_projection_lists_known_stores() {
        let versions = schema_versions();
        for name in [
            "control_plane",
            "task_runner",
            "app_registry",
            "config",
            "memory_sqlite",
            "session_sqlite",
            "backup_manifest",
        ] {
            assert!(
                versions.iter().any(|(n, v)| *n == name && *v > 0),
                "missing schema version entry for {name}: {versions:?}"
            );
        }
    }

    #[test]
    fn journaled_restore_swaps_all_stores_and_clears_the_journal() {
        let src = temp_dir("jsrc");
        let backup = temp_dir("jbackup");
        let target = temp_dir("jtarget");
        std::fs::write(src.join("control_plane.db"), b"cp").unwrap();
        // Ensure the nested app registry path exists for the manifest.
        std::fs::create_dir_all(src.join("apps")).unwrap();
        std::fs::write(src.join("apps").join("registry.json"), b"{\"apps\":[]}").unwrap();
        create_backup(&src, &backup, default_store_paths()).unwrap();

        assert_eq!(restore_backup_journaled(&backup, &target).unwrap(), 2);
        assert_eq!(
            std::fs::read(target.join("control_plane.db")).unwrap(),
            b"cp"
        );
        assert!(
            !target.join(RESTORE_JOURNAL).exists(),
            "journal must be cleared after a completed swap"
        );
        for dir in [src, backup, target] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn recover_pending_restore_finishes_a_crashed_swap() {
        let target = temp_dir("jrecover");
        let stage = target.join("control_plane.restore.pending");
        let final_path = target.join("control_plane.db");
        std::fs::write(&stage, b"new").unwrap();
        std::fs::write(&final_path, b"old").unwrap();
        std::fs::write(
            target.join(RESTORE_JOURNAL),
            serde_json::to_string(&vec![serde_json::json!({
                "stage": stage.to_string_lossy(),
                "final": final_path.to_string_lossy(),
            })])
            .unwrap(),
        )
        .unwrap();

        assert_eq!(recover_pending_restore(&target).unwrap(), 1);
        assert_eq!(std::fs::read(&final_path).unwrap(), b"new");
        assert!(!target.join(RESTORE_JOURNAL).exists());
        // Idempotent once the journal is gone.
        assert_eq!(recover_pending_restore(&target).unwrap(), 0);
        let _ = std::fs::remove_dir_all(&target);
    }
}