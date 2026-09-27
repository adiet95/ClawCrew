# Verified Fixes

### Fix #1 — Cancellation cascade preserves terminal children

| Field | Details |
| --- | --- |
| Date | 2026-09-24 |
| File | `crates/zeroclaw-runtime/src/control_plane/task_store_sqlite.rs` |
| Problem | Parent cancellation could mark already-terminal child tasks as requested. |
| Root cause | The recursive cascade filtered only on `cancellation_state = 'none'`, not terminal status. |
| Fix | Exclude completed, failed, cancelled, lost, and timed-out tasks from the cascade update. |
| Verification | `cargo test -p zeroclaw-runtime cancellation_is_idempotent_and_cascades_to_owned_children` passed. |
| Lesson | Cancellation propagation must preserve terminal lifecycle state. |
| Log Keyword | `cancellation cascade terminal child` |
| Deploy | BELUM DEPLOY |