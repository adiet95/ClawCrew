# Release Channels, Supported Versions, and Recovery

Status: initial · Updated: 2026-09-27

## Release channels

ClawCrew ships as a native binary (and a Tauri desktop bundle). There is a
single stable channel today; release artifacts are produced from tagged
`vX.Y.Z` commits. Pre-release tags carry a `-rc.N` suffix and are not
recommended for production.

- **stable** — tagged `vX.Y.Z`; the default.
- **rc** — tagged `vX.Y.Z-rc.N`; opt-in, may change before final.
- **nightly** — build-from-`main`; unsupported, no compatibility promise.

## Supported versions

`clawcrew_runtime::platform::backup::schema_versions()` is the canonical,
operator-readable list of the durable schema versions a build understands:

| Store | Meaning |
|---|---|
| `control_plane` | SQLite control-plane schema (`control_plane.db`) |
| `task_status_contract` | canonical task status enum |
| `task_runner` | TaskRunner workflow spec |
| `app_registry` | durable App install/enable state |
| `backup_manifest` | backup manifest contract |

An older on-disk version is migrated forward on open. A **newer** on-disk
version is refused (downgrade is unsafe) — see
`AppRegistry::open` and the control-plane schema migration.

## Backup

`clawcrew_runtime::platform::backup::create_backup(data_dir, dest, stores)`
copies the known durable stores into `dest` and writes `manifest.json`
(per-store size + SHA-256). `default_store_paths()` lists the stores currently
covered; add new durable store paths there as they land.

## Restore / rollback

- `plan_restore(backup_dir, target_root)` is a **dry run**: it verifies every
  backup file's digest and reports the `Create`/`Overwrite` actions a restore
  would take, plus a `compatible` flag. It touches nothing.
- `restore_backup(backup_dir, target_root)` applies a verified backup. Each
  file is written to a temporary and atomically renamed, so a crash mid-restore
  never leaves a half-written store.
- `restore_backup_journaled(backup_dir, target_root)` performs an
  **all-or-nothing cross-store swap**: every store is staged first, a
  `restore.journal` is written, then the staged files are swapped in.
  `recover_pending_restore(target_root)` finishes a swap that crashed mid-flight
  (call it at startup).
- A **rollback** is restoring a prior backup the same way — take a backup before
  an upgrade, and restore it if the upgrade fails.

Restore refuses an incompatible manifest, a missing file, a size mismatch, or a
digest mismatch (fail closed).

## Recovery procedures

1. Before an upgrade, run `create_backup` into a versioned directory.
2. After the upgrade, run `plan_restore` against the backup to confirm the
   manifest is intact and compatible.
3. If the upgrade fails, restore from the pre-upgrade backup (rollback).
4. If a store is corrupt, the control plane and App registry fail closed —
   records that do not parse are skipped, and a newer schema is refused rather
   than partially read.
