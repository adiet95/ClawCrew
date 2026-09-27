//! The single SQLite-backed [`TaskRegistry`] — EPIC A's durable index.

use std::path::Path;

use anyhow::{Context, Result};
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};

use super::authority::is_authoritative;
use super::task_registry::{
    RecoveryOutcome, TaskEvent, TaskKind, TaskRecord, TaskRegistry, TaskSnapshot, TaskStatus,
    TerminalSettlementIntent,
};

mod goal;
mod task_runner;

pub const CONTROL_PLANE_SCHEMA_VERSION: i64 = 12;
const MAX_STORED_TASK_EVENTS: i64 = 1_000;
const MAX_TASK_EVENT_PAYLOAD_BYTES: usize = 16 * 1024;

pub struct SqliteTaskStore {
    conn: Mutex<Connection>,
}

impl SqliteTaskStore {
    /// Open (creating if absent) the control-plane DB at `<data_dir>/control_plane.db`.
    /// Additive: a fresh install gets an empty DB and today's behavior is unchanged.
    pub fn new(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("create data dir {}", data_dir.display()))?;
        let db_path = data_dir.join("control_plane.db");
        let conn = Connection::open(&db_path)
            .with_context(|| format!("open control-plane DB: {}", db_path.display()))?;
        Self::init(conn)
    }

    /// In-memory store for unit tests.
    pub fn new_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory().context("open in-memory control-plane DB")?)
    }

    #[cfg(test)]
    pub(crate) fn insert_malformed_terminal_settlement_intent_for_test(
        &self,
        task_id: &str,
    ) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO terminal_settlement_intents
                (task_id, owner_pid, owner_boot_id, desired_status, artifact_path,
                 artifact_ref, artifact_sha256, terminal_error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                task_id,
                999_999_i64,
                "boot-OLD",
                "unknown-status",
                "/tmp/malformed-settlement.json",
                Option::<String>::None,
                "00".repeat(32),
                Option::<String>::None,
            ],
        )
        .context("insert malformed terminal settlement intent for test")?;
        Ok(())
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA temp_store = MEMORY;
             PRAGMA foreign_keys = ON;",
        )
        .context("set control-plane PRAGMAs")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tasks (
                 id              TEXT PRIMARY KEY,
                 kind            TEXT NOT NULL,
                 agent           TEXT NOT NULL,
                 status          TEXT NOT NULL,
                 owner_pid       INTEGER NOT NULL DEFAULT 0,
                 owner_boot_id   TEXT NOT NULL DEFAULT '',
                 heartbeat_at    TEXT,
                 depth           INTEGER NOT NULL DEFAULT 0,
                 parent_id       TEXT,
                 originator_route TEXT,
                 delivered       INTEGER NOT NULL DEFAULT 0,
                 idem_key        TEXT,
                 principal_id    TEXT,
                 started_at      TEXT NOT NULL,
                 finished_at     TEXT,
                 output          TEXT,
                 error           TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
             CREATE INDEX IF NOT EXISTS idx_tasks_agent  ON tasks(agent);
             CREATE INDEX IF NOT EXISTS idx_tasks_agent_kind_started
                ON tasks(agent, kind, started_at DESC);",
        )
        .context("create control-plane base schema")?;
        migrate_schema(&conn).context("migrate control-plane schema")?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Admin enumeration — count this agent's records (mirrors AcpSessionStore's
    /// `count_*_by_agent`; used by alias-delete cascades / observability).
    pub fn count_by_agent(&self, agent: &str) -> Result<u64> {
        let conn = self.conn.lock();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE agent = ?1",
                params![agent],
                |r| r.get(0),
            )
            .context("count tasks by agent")?;
        Ok(n as u64)
    }

    /// Admin enumeration — delete this agent's records (alias-delete cascade).
    pub fn delete_by_agent(&self, agent: &str) -> Result<u64> {
        let conn = self.conn.lock();
        let n = conn
            .execute("DELETE FROM tasks WHERE agent = ?1", params![agent])
            .context("delete tasks by agent")?;
        Ok(n as u64)
    }
}

fn migrate_schema(conn: &Connection) -> Result<()> {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .context("read control-plane schema version")?;
    goal::migrate_schema(conn, version)?;
    if version < 8 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS terminal_settlement_intents (
                 task_id          TEXT PRIMARY KEY
                                  REFERENCES tasks(id) ON DELETE CASCADE,
                 owner_pid        INTEGER NOT NULL,
                 owner_boot_id    TEXT NOT NULL,
                 desired_status   TEXT NOT NULL,
                 artifact_path    TEXT NOT NULL,
                 artifact_ref     TEXT,
                 artifact_sha256  TEXT NOT NULL,
                 terminal_error   TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_terminal_settlement_intents_owner
                ON terminal_settlement_intents(owner_pid, owner_boot_id);
             PRAGMA user_version = 8;",
        )
        .context("apply control-plane schema v8")?;
    }
    if version < 9 {
        add_column_if_missing(
            conn,
            "tasks",
            "session_key",
            "ALTER TABLE tasks ADD COLUMN session_key TEXT;",
        )?;
        add_column_if_missing(
            conn,
            "tasks",
            "workspace",
            "ALTER TABLE tasks ADD COLUMN workspace TEXT;",
        )?;
        add_column_if_missing(
            conn,
            "tasks",
            "cancellation_state",
            "ALTER TABLE tasks ADD COLUMN cancellation_state TEXT NOT NULL DEFAULT 'none';",
        )?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS task_events (
                 id              INTEGER PRIMARY KEY AUTOINCREMENT,
                 task_id         TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                 event_type      TEXT NOT NULL,
                 payload         TEXT NOT NULL,
                 timestamp       TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_task_events_task_id ON task_events(task_id);
             PRAGMA user_version = 9;",
        )
        .context("apply control-plane schema v9")?;
    }
    if version < 10 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS task_runner_specs (
                 task_id         TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
                 spec_payload    TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS task_runner_ledger (
                 task_id         TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                 step_id         TEXT NOT NULL,
                 timestamp       TEXT NOT NULL,
                 action          TEXT NOT NULL,
                 details         TEXT NOT NULL,
                 UNIQUE(task_id, step_id, action, timestamp)
             );
             CREATE INDEX IF NOT EXISTS idx_task_runner_ledger_task_id ON task_runner_ledger(task_id);
             PRAGMA user_version = 10;",
        )
        .context("apply control-plane schema v10")?;
    }
    if version < 11 {
        add_column_if_missing(
            conn,
            "tasks",
            "checkpoint_id",
            "ALTER TABLE tasks ADD COLUMN checkpoint_id TEXT;",
        )?;
        add_column_if_missing(
            conn,
            "tasks",
            "recovery_outcome",
            "ALTER TABLE tasks ADD COLUMN recovery_outcome TEXT NOT NULL DEFAULT 'fresh';",
        )?;
        conn.execute_batch("PRAGMA user_version = 11;")
            .context("apply control-plane schema v11")?;
    }
    if version < 12 {
        add_column_if_missing(
            conn,
            "task_runner_ledger",
            "correlation_id",
            "ALTER TABLE task_runner_ledger ADD COLUMN correlation_id TEXT;",
        )?;
        conn.execute_batch("PRAGMA user_version = 12;")
            .context("apply control-plane schema v12")?;
    }
    if version > CONTROL_PLANE_SCHEMA_VERSION {
        ::clawcrew_log::record!(
            WARN,
            ::clawcrew_log::Event::new(module_path!(), ::clawcrew_log::Action::Note).with_attrs(
                ::serde_json::json!({
                    "db_version": version,
                    "known_version": CONTROL_PLANE_SCHEMA_VERSION,
                })
            ),
            "control-plane DB was created by a newer schema version"
        );
    }
    Ok(())
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    alter_sql: &str,
) -> Result<()> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .with_context(|| format!("inspect {table} columns"))?;
    let mut rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .with_context(|| format!("query {table} columns"))?;
    let exists = rows.any(|name| matches!(name, Ok(name) if name == column));
    if !exists {
        conn.execute_batch(alter_sql)
            .with_context(|| format!("add {table}.{column}"))?;
    }
    Ok(())
}

// ── serde<->TEXT helpers (reuse the snake_case derive, no hand-kept string tables) ──

fn kind_to_db(k: TaskKind) -> String {
    serde_json::to_value(k)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "delegate".into())
}

fn status_to_db(s: TaskStatus) -> String {
    serde_json::to_value(s)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "running".into())
}

fn kind_from_db(s: &str) -> Result<TaskKind> {
    serde_json::from_value(serde_json::Value::String(s.to_owned()))
        .with_context(|| format!("unknown task kind {s:?}"))
}

fn status_from_db(s: &str) -> Result<TaskStatus> {
    serde_json::from_value(serde_json::Value::String(s.to_owned()))
        .with_context(|| format!("unknown task status {s:?}"))
}

fn cancellation_state_to_db(c: super::task_registry::CancellationState) -> String {
    serde_json::to_value(c)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "none".into())
}

fn cancellation_state_from_db(s: &str) -> Result<super::task_registry::CancellationState> {
    serde_json::from_value(serde_json::Value::String(s.to_owned()))
        .with_context(|| format!("unknown cancellation state {s:?}"))
}

fn recovery_outcome_to_db(outcome: RecoveryOutcome) -> String {
    serde_json::to_value(outcome)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "fresh".into())
}

fn recovery_outcome_from_db(s: &str) -> Result<RecoveryOutcome> {
    serde_json::from_value(serde_json::Value::String(s.to_owned()))
        .with_context(|| format!("unknown recovery outcome {s:?}"))
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRecord> {
    let kind_s: String = row.get("kind")?;
    let status_s: String = row.get("status")?;
    // serde parse failures map to a SQLite conversion error; callers SKIP such rows
    // (collect_skipping_bad_rows) rather than failing the whole query. The column index
    // (`0`) is a placeholder — rusqlite has no by-name conversion-error ctor and the
    // index is not surfaced to the skip path (review nit #4).
    let kind = kind_from_db(&kind_s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
    })?;
    let status = status_from_db(&status_s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
    })?;
    let cancellation_state_s: String = row
        .get("cancellation_state")
        .unwrap_or_else(|_| "none".to_string());
    let cancellation_state = cancellation_state_from_db(&cancellation_state_s)
        .unwrap_or(super::task_registry::CancellationState::None);
    let recovery_outcome_s: String = row
        .get("recovery_outcome")
        .unwrap_or_else(|_| "fresh".to_string());
    let recovery_outcome =
        recovery_outcome_from_db(&recovery_outcome_s).unwrap_or(RecoveryOutcome::Fresh);
    Ok(TaskRecord {
        id: row.get("id")?,
        kind,
        agent: row.get("agent")?,
        status,
        owner_pid: row.get::<_, i64>("owner_pid")? as u32,
        owner_boot_id: row.get("owner_boot_id")?,
        heartbeat_at: row.get("heartbeat_at")?,
        depth: row.get::<_, i64>("depth")? as u32,
        parent_id: row.get("parent_id")?,
        originator_route: row.get("originator_route")?,
        delivered: row.get::<_, i64>("delivered")? != 0,
        idem_key: row.get("idem_key")?,
        principal_id: row.get("principal_id")?,
        session_key: row.get("session_key").unwrap_or(None),
        workspace: row.get("workspace").unwrap_or(None),
        cancellation_state,
        checkpoint_id: row.get("checkpoint_id").unwrap_or(None),
        recovery_outcome,
        started_at: row.get("started_at")?,
        finished_at: row.get("finished_at")?,
    })
}

fn row_to_snapshot(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskSnapshot> {
    Ok(TaskSnapshot {
        task: row_to_record(row)?,
        output: row.get("output")?,
        error: row.get("error")?,
    })
}

fn row_to_settlement_intent(row: &rusqlite::Row<'_>) -> rusqlite::Result<TerminalSettlementIntent> {
    let status_s: String = row.get("desired_status")?;
    let desired_status = status_from_db(&status_s).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, error.into())
    })?;
    Ok(TerminalSettlementIntent {
        task_id: row.get("task_id")?,
        owner_pid: row.get::<_, i64>("owner_pid")? as u32,
        owner_boot_id: row.get("owner_boot_id")?,
        desired_status,
        artifact_path: row.get("artifact_path")?,
        artifact_ref: row.get("artifact_ref")?,
        artifact_sha256: row.get("artifact_sha256")?,
        terminal_error: row.get("terminal_error")?,
    })
}

fn validate_settlement_intent(intent: &TerminalSettlementIntent) -> Result<()> {
    anyhow::ensure!(
        intent.desired_status.is_terminal(),
        "terminal settlement intent for {} must use a terminal status",
        intent.task_id
    );
    anyhow::ensure!(
        !intent.artifact_path.is_empty(),
        "terminal settlement intent for {} has no artifact path",
        intent.task_id
    );
    if intent.desired_status == TaskStatus::Completed {
        anyhow::ensure!(
            intent
                .artifact_ref
                .as_deref()
                .is_some_and(|artifact_ref| !artifact_ref.is_empty()),
            "completed settlement intent for {} has no artifact reference",
            intent.task_id
        );
    }
    anyhow::ensure!(
        hex::decode(&intent.artifact_sha256)
            .map(|digest| digest.len() == 32)
            .unwrap_or(false),
        "terminal settlement intent for {} has an invalid SHA-256 digest",
        intent.task_id
    );
    Ok(())
}

fn delete_settlement_intent_record(
    conn: &Connection,
    intent: &TerminalSettlementIntent,
) -> Result<usize> {
    conn.execute(
        "DELETE FROM terminal_settlement_intents
          WHERE task_id = ?1
            AND owner_pid = ?2
            AND owner_boot_id = ?3
            AND desired_status = ?4
            AND artifact_path = ?5
            AND artifact_ref IS ?6
            AND artifact_sha256 = ?7
            AND terminal_error IS ?8",
        params![
            &intent.task_id,
            intent.owner_pid as i64,
            &intent.owner_boot_id,
            status_to_db(intent.desired_status),
            &intent.artifact_path,
            &intent.artifact_ref,
            &intent.artifact_sha256,
            &intent.terminal_error,
        ],
    )
    .context("delete terminal settlement intent")
}

fn persist_settlement_intent_record(
    conn: &mut Connection,
    intent: &TerminalSettlementIntent,
) -> Result<bool> {
    validate_settlement_intent(intent)?;
    let tx = conn
        .transaction()
        .context("begin settlement intent transaction")?;
    let task_is_active: bool = tx
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM tasks
                 WHERE id = ?1
                   AND owner_pid = ?2
                   AND owner_boot_id = ?3
                   AND status NOT IN ('completed','failed','cancelled','lost','timed_out')
            )",
            params![
                &intent.task_id,
                intent.owner_pid as i64,
                &intent.owner_boot_id
            ],
            |row| row.get::<_, i64>(0),
        )
        .context("check terminal settlement owner")?
        != 0;
    if !task_is_active {
        tx.commit()
            .context("finish inactive settlement intent check")?;
        return Ok(false);
    }

    let inserted = tx
        .execute(
            "INSERT INTO terminal_settlement_intents
                (task_id, owner_pid, owner_boot_id, desired_status, artifact_path,
                 artifact_ref, artifact_sha256, terminal_error)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(task_id) DO NOTHING",
            params![
                &intent.task_id,
                intent.owner_pid as i64,
                &intent.owner_boot_id,
                status_to_db(intent.desired_status),
                &intent.artifact_path,
                &intent.artifact_ref,
                &intent.artifact_sha256,
                &intent.terminal_error,
            ],
        )
        .context("persist terminal settlement intent")?;
    if inserted == 1 {
        tx.commit().context("commit terminal settlement intent")?;
        return Ok(true);
    }

    let existing = tx
        .query_row(
            "SELECT * FROM terminal_settlement_intents WHERE task_id = ?1",
            params![&intent.task_id],
            row_to_settlement_intent,
        )
        .optional()
        .context("read existing terminal settlement intent")?;
    if let Some(existing) = existing {
        anyhow::ensure!(
            existing == intent.clone(),
            "conflicting terminal settlement intent for task {}",
            intent.task_id
        );
        tx.commit()
            .context("commit existing terminal settlement intent")?;
        return Ok(true);
    }

    tx.commit()
        .context("finish missing settlement intent check")?;
    Ok(false)
}

fn promote_settlement_record(
    conn: &mut Connection,
    intent: &TerminalSettlementIntent,
    resolved_status: TaskStatus,
    output: Option<String>,
    error: Option<String>,
) -> Result<bool> {
    anyhow::ensure!(
        resolved_status.is_terminal(),
        "terminal settlement resolution requires a terminal status"
    );
    let tx = conn
        .transaction()
        .context("begin terminal settlement promotion")?;
    let finished_at = chrono::Utc::now().to_rfc3339();
    let changed = tx
        .execute(
            "UPDATE tasks
                SET status = ?1,
                    output = ?2,
                    error = ?3,
                    recovery_outcome = CASE
                        WHEN ?3 IS NOT NULL THEN 'needs_review'
                        ELSE recovery_outcome
                    END,
                    finished_at = ?4
              WHERE id = ?5
                AND owner_pid = ?6
                AND owner_boot_id = ?7
                AND status NOT IN ('completed','failed','cancelled','lost','timed_out')
                AND EXISTS (
                    SELECT 1
                      FROM terminal_settlement_intents
                     WHERE task_id = ?5
                       AND owner_pid = ?6
                       AND owner_boot_id = ?7
                       AND desired_status = ?8
                       AND artifact_path = ?9
                       AND artifact_ref IS ?10
                       AND artifact_sha256 = ?11
                       AND terminal_error IS ?12
                )",
            params![
                status_to_db(resolved_status),
                output,
                error,
                finished_at,
                &intent.task_id,
                intent.owner_pid as i64,
                &intent.owner_boot_id,
                status_to_db(intent.desired_status),
                &intent.artifact_path,
                &intent.artifact_ref,
                &intent.artifact_sha256,
                &intent.terminal_error,
            ],
        )
        .context("promote terminal settlement")?;
    let _ = delete_settlement_intent_record(&tx, intent)?;
    tx.commit()
        .context("commit terminal settlement promotion")?;
    Ok(changed == 1)
}

/// Collect query rows, SKIPPING (and logging) any single row that fails to convert —
/// one unrecognised/corrupt record (e.g. a forward-incompat `kind`/`status` written by a
/// newer binary) must not fail the whole enumeration and starve the reaper (finding #3).
fn collect_skipping_bad_rows<I>(rows: I) -> Vec<TaskRecord>
where
    I: Iterator<Item = rusqlite::Result<TaskRecord>>,
{
    let mut out = Vec::new();
    for r in rows {
        match r {
            Ok(rec) => out.push(rec),
            Err(e) => log_unreadable_task_row(e),
        }
    }
    out
}

fn log_unreadable_task_row(error: rusqlite::Error) {
    ::clawcrew_log::record!(
        WARN,
        ::clawcrew_log::Event::new(module_path!(), ::clawcrew_log::Action::Note)
            .with_outcome(::clawcrew_log::EventOutcome::Unknown)
            .with_attrs(::serde_json::json!({ "error": format!("{error}") })),
        "control-plane: skipping unreadable task row"
    );
}

/// Collect settlement intents while skipping a corrupt persisted row. Recovery
/// metadata must not keep ordinary task reconciliation from running.
fn collect_skipping_bad_settlement_intents<I>(rows: I) -> Vec<TerminalSettlementIntent>
where
    I: Iterator<Item = rusqlite::Result<TerminalSettlementIntent>>,
{
    let mut out = Vec::new();
    for row in rows {
        match row {
            Ok(intent) => out.push(intent),
            Err(error) => log_unreadable_terminal_settlement_intent(error),
        }
    }
    out
}

fn log_unreadable_terminal_settlement_intent(error: rusqlite::Error) {
    ::clawcrew_log::record!(
        WARN,
        ::clawcrew_log::Event::new(module_path!(), ::clawcrew_log::Action::Note)
            .with_outcome(::clawcrew_log::EventOutcome::Unknown)
            .with_attrs(::serde_json::json!({ "error": format!("{error}") })),
        "control-plane: skipping unreadable terminal settlement intent"
    );
}

fn redact_task_event_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(object) => serde_json::Value::Object(
            object
                .into_iter()
                .map(|(key, value)| {
                    let lower = key.to_ascii_lowercase();
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
                    .any(|part| lower.contains(part));
                    (
                        key,
                        if sensitive {
                            serde_json::Value::String("[REDACTED]".into())
                        } else {
                            redact_task_event_value(value)
                        },
                    )
                })
                .collect(),
        ),
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(redact_task_event_value).collect())
        }
        other => other,
    }
}

fn bounded_task_event_payload(payload: &serde_json::Value) -> serde_json::Value {
    let redacted = redact_task_event_value(payload.clone());
    if serde_json::to_vec(&redacted)
        .map(|bytes| bytes.len() <= MAX_TASK_EVENT_PAYLOAD_BYTES)
        .unwrap_or(false)
    {
        redacted
    } else {
        serde_json::json!({
            "redacted": true,
            "reason": "event payload exceeds control-plane limit"
        })
    }
}

fn insert_task_record(conn: &Connection, rec: TaskRecord) -> Result<()> {
    if let Some(idem) = &rec.idem_key {
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM tasks WHERE idem_key = ?1 AND principal_id = ?2 AND session_key = ?3)",
                params![idem, rec.principal_id, rec.session_key],
                |row| row.get(0),
            )
            .unwrap_or(false);
        anyhow::ensure!(
            !exists,
            "duplicate task side effect prevented by idempotency key: {}",
            idem
        );
    }
    // ON CONFLICT DO NOTHING, NOT INSERT OR REPLACE: re-registering an existing id
    // must be a true no-op, never clobber an already-recorded output/error/terminal
    // status back to NULL/running (review finding— the documented idempotency).
    conn.execute(
        "INSERT INTO tasks
            (id, kind, agent, status, owner_pid, owner_boot_id, heartbeat_at, depth,
             parent_id, originator_route, delivered, idem_key, principal_id,
             session_key, workspace, cancellation_state, checkpoint_id,
             recovery_outcome, started_at, finished_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)
         ON CONFLICT(id) DO NOTHING",
        params![
            rec.id,
            kind_to_db(rec.kind),
            rec.agent,
            status_to_db(rec.status),
            rec.owner_pid as i64,
            rec.owner_boot_id,
            rec.heartbeat_at,
            rec.depth as i64,
            rec.parent_id,
            rec.originator_route,
            rec.delivered as i64,
            rec.idem_key,
            rec.principal_id,
            rec.session_key,
            rec.workspace,
            cancellation_state_to_db(rec.cancellation_state),
            rec.checkpoint_id,
            recovery_outcome_to_db(rec.recovery_outcome),
            rec.started_at,
            rec.finished_at,
        ],
    )
    .context("insert task record")?;
    Ok(())
}

fn update_task_status_record(
    conn: &Connection,
    id: &str,
    status: TaskStatus,
    output: Option<String>,
    error: Option<String>,
) -> Result<usize> {
    let finished_at = status
        .is_terminal()
        .then(|| chrono::Utc::now().to_rfc3339());
    let changed = conn
        .execute(
            "UPDATE tasks
                SET status = ?1,
                    output = COALESCE(?2, output),
                    error  = COALESCE(?3, error),
                    finished_at = COALESCE(?4, finished_at)
              WHERE id = ?5
                AND status NOT IN ('completed','failed','cancelled','lost','timed_out')",
            params![status_to_db(status), output, error, finished_at, id],
        )
        .context("update task status")?;
    if changed > 0 && status.is_terminal() {
        conn.execute(
            "DELETE FROM terminal_settlement_intents WHERE task_id = ?1",
            params![id],
        )
        .context("delete stale terminal settlement intent")?;
    }
    Ok(changed)
}

fn transition_task_terminal_record(
    conn: &mut Connection,
    id: &str,
    status: TaskStatus,
    output: Option<String>,
    error: Option<String>,
) -> Result<usize> {
    anyhow::ensure!(
        status.is_terminal(),
        "terminal transition requires a terminal status"
    );
    let tx = conn.transaction().context("begin terminal transition")?;
    let finished_at = chrono::Utc::now().to_rfc3339();
    let changed = tx
        .execute(
            "UPDATE tasks
                SET status = ?1,
                    output = ?2,
                    error = ?3,
                    finished_at = ?4
              WHERE id = ?5
                AND status NOT IN ('completed','failed','cancelled','lost','timed_out')",
            params![status_to_db(status), output, error, finished_at, id],
        )
        .context("transition task terminal")?;
    if changed == 1 {
        tx.execute(
            "DELETE FROM terminal_settlement_intents WHERE task_id = ?1",
            params![id],
        )
        .context("delete stale terminal settlement intent")?;
    }
    tx.commit().context("commit terminal transition")?;
    Ok(changed)
}

fn transition_task_terminal_if_owner_record(
    conn: &mut Connection,
    id: &str,
    owner_pid: u32,
    owner_boot_id: &str,
    status: TaskStatus,
    output: Option<String>,
    error: Option<String>,
) -> Result<usize> {
    anyhow::ensure!(
        status.is_terminal(),
        "terminal transition requires a terminal status"
    );
    let tx = conn
        .transaction()
        .context("begin owner-checked terminal transition")?;
    let finished_at = chrono::Utc::now().to_rfc3339();
    let changed = tx
        .execute(
            "UPDATE tasks
                SET status = ?1,
                    output = ?2,
                    error = ?3,
                    finished_at = ?4
              WHERE id = ?5
                AND owner_pid = ?6
                AND owner_boot_id = ?7
                AND status NOT IN ('completed','failed','cancelled','lost','timed_out')",
            params![
                status_to_db(status),
                output,
                error,
                finished_at,
                id,
                owner_pid as i64,
                owner_boot_id,
            ],
        )
        .context("transition owner-checked task terminal")?;
    if changed == 1 {
        tx.execute(
            "DELETE FROM terminal_settlement_intents WHERE task_id = ?1",
            params![id],
        )
        .context("delete stale terminal settlement intent")?;
    }
    tx.commit()
        .context("commit owner-checked terminal transition")?;
    Ok(changed)
}

fn claim_task_owner_record(
    conn: &Connection,
    id: &str,
    owner_pid: u32,
    owner_boot_id: &str,
) -> Result<usize> {
    conn.execute(
        "UPDATE tasks
            SET owner_pid = ?1,
                owner_boot_id = ?2,
                heartbeat_at = NULL,
                recovery_outcome = 'resumed'
          WHERE id = ?3
            AND status NOT IN ('completed','failed','cancelled','lost','timed_out')",
        params![owner_pid as i64, owner_boot_id, id],
    )
    .context("claim task owner")
}

fn request_task_cancellation_record(conn: &Connection, id: &str) -> Result<usize> {
    conn.execute(
        "UPDATE tasks
            SET cancellation_state = 'requested'
          WHERE id = ?1
            AND status NOT IN ('completed','failed','cancelled','lost','timed_out')
            AND cancellation_state = 'none'",
        params![id],
    )
    .context("request task cancellation")
}

fn acknowledge_task_cancellation_record(
    conn: &Connection,
    id: &str,
    owner_pid: u32,
    owner_boot_id: &str,
) -> Result<usize> {
    conn.execute(
        "UPDATE tasks
            SET cancellation_state = 'acknowledged'
          WHERE id = ?1
            AND owner_pid = ?2
            AND owner_boot_id = ?3
            AND status NOT IN ('completed','failed','cancelled','lost','timed_out')
            AND cancellation_state = 'requested'",
        params![id, owner_pid as i64, owner_boot_id],
    )
    .context("acknowledge task cancellation")
}

#[async_trait::async_trait]
impl TaskRegistry for SqliteTaskStore {
    async fn create(&self, rec: TaskRecord) -> Result<()> {
        let conn = self.conn.lock();
        insert_task_record(&conn, rec)?;
        Ok(())
    }

    async fn heartbeat(&self, id: &str, owner_boot_id: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.conn.lock();
        // Only the heart-beating owner refreshes; prevents a stale boot from
        // resurrecting liveness it does not own.
        conn.execute(
            "UPDATE tasks SET heartbeat_at = ?1
             WHERE id = ?2 AND owner_boot_id = ?3",
            params![now, id, owner_boot_id],
        )
        .context("heartbeat task")?;
        Ok(())
    }

    async fn update_checkpoint(
        &self,
        id: &str,
        owner_boot_id: &str,
        checkpoint_id: Option<String>,
    ) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE tasks SET checkpoint_id = ?1
             WHERE id = ?2
               AND owner_boot_id = ?3
               AND status NOT IN ('completed','failed','cancelled','lost','timed_out')",
            params![checkpoint_id, id, owner_boot_id],
        )
        .context("update task checkpoint")?;
        Ok(())
    }

    async fn update_status(
        &self,
        id: &str,
        status: TaskStatus,
        output: Option<String>,
        error: Option<String>,
    ) -> Result<()> {
        let conn = self.conn.lock();
        update_task_status_record(&conn, id, status, output, error)?;
        Ok(())
    }

    async fn transition_terminal(
        &self,
        id: &str,
        status: TaskStatus,
        output: Option<String>,
        error: Option<String>,
    ) -> Result<bool> {
        let mut conn = self.conn.lock();
        Ok(transition_task_terminal_record(&mut conn, id, status, output, error)? == 1)
    }

    async fn transition_terminal_if_owner(
        &self,
        id: &str,
        owner_pid: u32,
        owner_boot_id: &str,
        status: TaskStatus,
        output: Option<String>,
        error: Option<String>,
    ) -> Result<bool> {
        let mut conn = self.conn.lock();
        Ok(transition_task_terminal_if_owner_record(
            &mut conn,
            id,
            owner_pid,
            owner_boot_id,
            status,
            output,
            error,
        )? == 1)
    }

    async fn claim(&self, id: &str, owner_pid: u32, owner_boot_id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        Ok(claim_task_owner_record(&conn, id, owner_pid, owner_boot_id)? == 1)
    }

    async fn transfer_ownership(
        &self,
        id: &str,
        new_owner_pid: u32,
        new_owner_boot_id: &str,
        auth_token: &str,
    ) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction().context("begin ownership transfer")?;

        let timestamp = chrono::Utc::now().to_rfc3339();
        let payload = serde_json::json!({
            "new_owner_pid": new_owner_pid,
            "new_owner_boot_id": new_owner_boot_id,
            "auth_token": auth_token,
        });

        let payload_str = serde_json::to_string(&bounded_task_event_payload(&payload)).unwrap();
        tx.execute(
            "INSERT INTO task_events (task_id, event_type, payload, timestamp) VALUES (?1, ?2, ?3, ?4)",
            params![id, "ownership_transferred", payload_str, timestamp],
        ).context("insert ownership transfer event")?;

        let changed = tx
            .execute(
                "UPDATE tasks
                SET owner_pid = ?1,
                    owner_boot_id = ?2,
                    heartbeat_at = NULL,
                    recovery_outcome = 'resumed'
              WHERE id = ?3
                AND status NOT IN ('completed','failed','cancelled','lost','timed_out')",
                params![new_owner_pid as i64, new_owner_boot_id, id],
            )
            .context("transfer task ownership")?;

        if changed == 0 {
            anyhow::bail!("task cannot be transferred (terminal or missing)");
        }

        tx.execute(
            "DELETE FROM terminal_settlement_intents WHERE task_id = ?1",
            params![id],
        )
        .context("delete prior-owner terminal settlement intent")?;

        tx.commit().context("commit ownership transfer")?;
        Ok(())
    }

    async fn aggregate_child_result(
        &self,
        parent_id: &str,
        child_id: &str,
        child_status: TaskStatus,
        child_output: Option<String>,
    ) -> Result<bool> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction().context("begin aggregate child result")?;

        let delivered = tx
            .query_row(
                "SELECT delivered FROM tasks WHERE id = ?1 AND parent_id = ?2",
                params![child_id, parent_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .context("check child delivery status")?;

        let Some(delivered) = delivered else {
            return Ok(false);
        };
        if delivered != 0 {
            return Ok(false);
        }

        tx.execute(
            "UPDATE tasks SET delivered = 1 WHERE id = ?1",
            params![child_id],
        )
        .context("mark child delivered")?;

        let timestamp = chrono::Utc::now().to_rfc3339();
        let payload = serde_json::json!({
            "child_id": child_id,
            "child_status": status_to_db(child_status),
            "child_output": child_output,
        });
        let payload_str = serde_json::to_string(&bounded_task_event_payload(&payload)).unwrap();
        tx.execute(
            "INSERT INTO task_events (task_id, event_type, payload, timestamp) VALUES (?1, ?2, ?3, ?4)",
            params![parent_id, "child_aggregated", payload_str, timestamp],
        ).context("insert aggregation event")?;

        tx.commit().context("commit child aggregation")?;
        Ok(true)
    }

    async fn request_cancellation(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        Ok(request_task_cancellation_record(&conn, id)? == 1)
    }

    async fn acknowledge_cancellation(
        &self,
        id: &str,
        owner_pid: u32,
        owner_boot_id: &str,
    ) -> Result<bool> {
        let conn = self.conn.lock();
        Ok(acknowledge_task_cancellation_record(&conn, id, owner_pid, owner_boot_id)? == 1)
    }

    async fn cascade_cancellation(&self, parent_id: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "WITH RECURSIVE
               descendants(id) AS (
                 SELECT id FROM tasks WHERE parent_id = ?1
                 UNION ALL
                 SELECT t.id FROM tasks t JOIN descendants d ON t.parent_id = d.id
               )
             UPDATE tasks
             SET cancellation_state = 'requested'
                         WHERE id IN descendants
                             AND status NOT IN ('completed','failed','cancelled','lost','timed_out')
                             AND cancellation_state = 'none'",
            params![parent_id],
        )?;
        Ok(())
    }

    async fn get_child_tasks(&self, parent_id: &str) -> Result<Vec<TaskRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT * FROM tasks WHERE parent_id = ?1")?;
        let mut rows = stmt.query(params![parent_id])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(row_to_record(row)?);
        }
        Ok(out)
    }

    async fn list_descendants(&self, root_id: &str) -> Result<Vec<TaskRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "WITH RECURSIVE descendants(id) AS (
                     SELECT id FROM tasks WHERE parent_id = ?1
                     UNION
                     SELECT t.id FROM tasks t JOIN descendants d ON t.parent_id = d.id
                 )
                 SELECT * FROM tasks WHERE id IN descendants",
            )
            .context("prepare list_descendants")?;
        let rows = stmt
            .query_map(params![root_id], row_to_record)
            .context("query list_descendants")?;
        Ok(collect_skipping_bad_rows(rows))
    }

    async fn record_task_event(
        &self,
        task_id: &str,
        event_type: &str,
        payload: &serde_json::Value,
    ) -> Result<()> {
        let conn = self.conn.lock();
        let payload_str = serde_json::to_string(&bounded_task_event_payload(payload))
            .context("serialize redacted task event")?;
        let timestamp = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO task_events (task_id, event_type, payload, timestamp)
             VALUES (?1, ?2, ?3, ?4)",
            params![task_id, event_type, payload_str, timestamp],
        )
        .context("insert task event")?;
        conn.execute(
            "DELETE FROM task_events
             WHERE task_id = ?1
               AND id NOT IN (
                   SELECT id FROM task_events
                   WHERE task_id = ?1
                   ORDER BY id DESC
                   LIMIT ?2
               )",
            params![task_id, MAX_STORED_TASK_EVENTS],
        )
        .context("prune task event history")?;
        Ok(())
    }

    async fn list_task_events(
        &self,
        task_id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<TaskEvent>> {
        let limit = i64::from(limit.clamp(1, 100));
        let offset = i64::from(offset);
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, task_id, event_type, payload, timestamp
                 FROM task_events
                 WHERE task_id = ?1
                 ORDER BY id DESC
                 LIMIT ?2 OFFSET ?3",
            )
            .context("prepare list task events")?;
        let rows = stmt.query_map(params![task_id, limit, offset], |row| {
            let payload_text: String = row.get(3)?;
            Ok(TaskEvent {
                id: row.get(0)?,
                task_id: row.get(1)?,
                event_type: row.get(2)?,
                payload: serde_json::from_str(&payload_text).unwrap_or(serde_json::Value::Null),
                timestamp: row.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("read task events")
    }

    async fn list_task_ledger(
        &self,
        task_id: &str,
    ) -> Result<Vec<crate::control_plane::task_runner::LedgerEntry>> {
        self.get_ledger_entries(task_id)
            .context("read task ledger for projection")
    }

    async fn persist_terminal_settlement_intent(
        &self,
        intent: TerminalSettlementIntent,
    ) -> Result<bool> {
        let mut conn = self.conn.lock();
        persist_settlement_intent_record(&mut conn, &intent)
    }

    async fn list_terminal_settlement_intents(&self) -> Result<Vec<TerminalSettlementIntent>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT * FROM terminal_settlement_intents
                 ORDER BY task_id",
            )
            .context("prepare list terminal settlement intents")?;
        let rows = stmt
            .query_map([], row_to_settlement_intent)
            .context("query terminal settlement intents")?;
        Ok(collect_skipping_bad_settlement_intents(rows))
    }

    async fn promote_terminal_settlement(
        &self,
        intent: &TerminalSettlementIntent,
        resolved_status: TaskStatus,
        output: Option<String>,
        error: Option<String>,
    ) -> Result<bool> {
        let mut conn = self.conn.lock();
        promote_settlement_record(&mut conn, intent, resolved_status, output, error)
    }

    async fn discard_terminal_settlement_intent(
        &self,
        intent: &TerminalSettlementIntent,
    ) -> Result<bool> {
        let conn = self.conn.lock();
        Ok(delete_settlement_intent_record(&conn, intent)? == 1)
    }

    async fn reopen_for_operator(&self, id: &str) -> Result<bool> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction().context("begin operator reopen")?;
        let finished_at_reset = tx
            .execute(
                "UPDATE tasks
                    SET status = 'running',
                        recovery_outcome = 'resumed',
                        finished_at = NULL,
                        error = NULL
                  WHERE id = ?1
                    AND status IN ('paused','needs_review','failed')",
                params![id],
            )
            .context("operator reopen task")?;
        if finished_at_reset == 1 {
            let payload = serde_json::to_string(&bounded_task_event_payload(
                &serde_json::json!({ "by": "operator" }),
            ))
            .context("serialize reopen event")?;
            tx.execute(
                "INSERT INTO task_events (task_id, event_type, payload, timestamp)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    id,
                    "operator_reopened",
                    payload,
                    chrono::Utc::now().to_rfc3339()
                ],
            )
            .context("insert operator reopen event")?;
            tx.execute(
                "DELETE FROM terminal_settlement_intents WHERE task_id = ?1",
                params![id],
            )
            .context("delete stale settlement intent on reopen")?;
        }
        tx.commit().context("commit operator reopen")?;
        Ok(finished_at_reset == 1)
    }

    async fn claim_owner(&self, id: &str, owner_pid: u32, owner_boot_id: &str) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction().context("begin task owner claim")?;
        tx.execute(
            "DELETE FROM terminal_settlement_intents
              WHERE task_id = ?1
                AND (owner_pid != ?2 OR owner_boot_id != ?3)",
            params![id, owner_pid as i64, owner_boot_id],
        )
        .context("delete prior-owner terminal settlement intent")?;
        claim_task_owner_record(&tx, id, owner_pid, owner_boot_id)?;
        tx.commit().context("commit task owner claim")?;
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<TaskRecord>> {
        let conn = self.conn.lock();
        let rec = conn
            .query_row(
                "SELECT * FROM tasks WHERE id = ?1",
                params![id],
                row_to_record,
            )
            .optional()
            .context("get task")?;
        Ok(rec)
    }

    async fn get_snapshot(&self, id: &str) -> Result<Option<TaskSnapshot>> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT * FROM tasks WHERE id = ?1",
            params![id],
            row_to_snapshot,
        )
        .optional()
        .context("get task snapshot")
    }

    async fn list_running(&self) -> Result<Vec<TaskRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT * FROM tasks WHERE status = 'running'")
            .context("prepare list_running")?;
        let rows = stmt
            .query_map([], row_to_record)
            .context("query list_running")?;
        Ok(collect_skipping_bad_rows(rows))
    }

    async fn list_all(&self) -> Result<Vec<TaskRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT * FROM tasks ORDER BY started_at DESC")
            .context("prepare list_all")?;
        let rows = stmt
            .query_map([], row_to_record)
            .context("query list_all")?;
        Ok(collect_skipping_bad_rows(rows))
    }

    async fn list_by_agent(&self, agent: &str) -> Result<Vec<TaskRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT * FROM tasks WHERE agent = ?1 ORDER BY started_at DESC")
            .context("prepare list_by_agent")?;
        let rows = stmt
            .query_map(params![agent], row_to_record)
            .context("query list_by_agent")?;
        Ok(collect_skipping_bad_rows(rows))
    }

    async fn reconcile_lost(&self, id: &str, _now_boot_id: &str) -> Result<bool> {
        let rec = {
            let conn = self.conn.lock();
            conn.query_row(
                "SELECT * FROM tasks WHERE id = ?1",
                params![id],
                row_to_record,
            )
            .optional()
            .context("reconcile: load task")?
        };
        let Some(rec) = rec else { return Ok(false) };
        // Never reclaim a terminal record, and never one a live owner still holds.
        if rec.status.is_terminal() || !is_authoritative(&rec) {
            return Ok(false);
        }
        let now = chrono::Utc::now().to_rfc3339();
        let mut conn = self.conn.lock();
        let tx = conn
            .transaction()
            .context("reconcile: begin lost transition")?;
        let changed = tx
            .execute(
                "UPDATE tasks
                    SET status = 'lost',
                        recovery_outcome = 'lost',
                        error = COALESCE(error, 'task owner is no longer available'),
                        finished_at = ?1
                  WHERE id = ?2
                    AND status = 'running'
                    AND owner_pid = ?3
                    AND owner_boot_id = ?4",
                params![now, id, rec.owner_pid as i64, rec.owner_boot_id],
            )
            .context("reconcile: mark lost")?;
        if changed == 1 {
            tx.execute(
                "DELETE FROM terminal_settlement_intents WHERE task_id = ?1",
                params![id],
            )
            .context("reconcile: delete stale terminal settlement intent")?;
        }
        tx.commit().context("reconcile: commit lost transition")?;
        Ok(changed == 1)
    }

    async fn reconcile_timed_out(
        &self,
        id: &str,
        owner_pid: u32,
        owner_boot_id: &str,
        heartbeat_at: &str,
    ) -> Result<bool> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut conn = self.conn.lock();
        let tx = conn
            .transaction()
            .context("reconcile: begin timeout transition")?;
        let changed = tx
            .execute(
                "UPDATE tasks
                    SET status = 'timed_out',
                        recovery_outcome = 'needs_review',
                        error = COALESCE(error, 'heartbeat timeout'),
                        finished_at = ?1
                  WHERE id = ?2
                    AND status = 'running'
                    AND owner_pid = ?3
                    AND owner_boot_id = ?4
                    AND heartbeat_at = ?5",
                params![now, id, owner_pid as i64, owner_boot_id, heartbeat_at],
            )
            .context("reconcile: mark timed out")?;
        if changed == 1 {
            tx.execute(
                "DELETE FROM terminal_settlement_intents WHERE task_id = ?1",
                params![id],
            )
            .context("reconcile: delete stale terminal settlement intent")?;
        }
        tx.commit()
            .context("reconcile: commit timeout transition")?;
        Ok(changed == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_plane::task_registry::CancellationState;

    fn rec(id: &str, agent: &str, owner_pid: u32, boot: &str) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            kind: TaskKind::Delegate,
            agent: agent.into(),
            status: TaskStatus::Running,
            owner_pid,
            owner_boot_id: boot.into(),
            heartbeat_at: None,
            depth: 0,
            parent_id: None,
            originator_route: None,
            delivered: false,
            idem_key: None,
            principal_id: None,
            session_key: None,
            workspace: None,
            cancellation_state: crate::control_plane::task_registry::CancellationState::None,
            checkpoint_id: None,
            recovery_outcome: Default::default(),
            started_at: "2026-06-18T00:00:00Z".into(),
            finished_at: None,
        }
    }

    #[tokio::test]
    async fn list_descendants_returns_whole_subtree() {
        let s = SqliteTaskStore::new_in_memory().unwrap();
        s.create(rec("root", "main", 1, "b")).await.unwrap();
        let mut a = rec("a", "main", 1, "b");
        a.parent_id = Some("root".into());
        let mut b = rec("b", "main", 1, "b");
        b.parent_id = Some("a".into());
        let mut c = rec("c", "main", 1, "b");
        c.parent_id = Some("root".into());
        s.create(a).await.unwrap();
        s.create(b).await.unwrap();
        s.create(c).await.unwrap();

        let mut ids: Vec<String> = s
            .list_descendants("root")
            .await
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        ids.sort();
        assert_eq!(ids, vec!["a", "b", "c"]);
        // Direct children only sees the first hop.
        let direct = s.get_child_tasks("root").await.unwrap();
        assert_eq!(direct.len(), 2, "direct children are a and c");
        // A leaf has no descendants.
        assert!(s.list_descendants("b").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn operator_reopen_only_from_reopenable_states() {
        let s = SqliteTaskStore::new_in_memory().unwrap();
        for (id, status) in [
            ("p", TaskStatus::Paused),
            ("nr", TaskStatus::NeedsReview),
            ("f", TaskStatus::Failed),
            ("c", TaskStatus::Completed),
            ("x", TaskStatus::Cancelled),
        ] {
            s.create(rec(id, "main", 1, "b")).await.unwrap();
            s.update_status(id, status, None, None).await.unwrap();
        }

        assert!(s.reopen_for_operator("p").await.unwrap());
        assert_eq!(s.get("p").await.unwrap().unwrap().status, TaskStatus::Running);
        // A running task is not reopenable.
        assert!(!s.reopen_for_operator("p").await.unwrap());
        assert!(s.reopen_for_operator("nr").await.unwrap());
        assert!(s.reopen_for_operator("f").await.unwrap());
        // Terminal success/cancel are never reopened.
        assert!(!s.reopen_for_operator("c").await.unwrap());
        assert!(!s.reopen_for_operator("x").await.unwrap());

        let events = s.list_task_events("p", 10, 0).await.unwrap();
        assert!(events.iter().any(|e| e.event_type == "operator_reopened"));
    }

    #[tokio::test]
    async fn list_task_ledger_projects_ordered_entries() {
        use crate::control_plane::task_runner::{LedgerAction, LedgerEntry};
        let s = SqliteTaskStore::new_in_memory().unwrap();
        s.create(rec("t", "main", 1, "b")).await.unwrap();
        for (ts, action) in [
            ("2026-01-01T00:00:00Z", LedgerAction::Started),
            ("2026-01-01T00:00:01Z", LedgerAction::Completed),
        ] {
            s.append_ledger_entry(&LedgerEntry {
                task_id: "t".into(),
                step_id: "step-1".into(),
                timestamp: ts.into(),
                action,
                details: "evidence".into(),
                correlation_id: Some("corr-1".into()),
            })
            .unwrap();
        }
        let entries = s.list_task_ledger("t").await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].timestamp, "2026-01-01T00:00:00Z");
        assert_eq!(entries[0].correlation_id.as_deref(), Some("corr-1"));
        assert!(s.list_task_ledger("missing").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_get_roundtrip() {
        let s = SqliteTaskStore::new_in_memory().unwrap();
        s.create(rec("a", "main", 1, "boot-1")).await.unwrap();
        let got = s.get("a").await.unwrap().unwrap();
        assert_eq!(got.id, "a");
        assert_eq!(got.kind, TaskKind::Delegate);
        assert_eq!(got.status, TaskStatus::Running);
        assert_eq!(got.recovery_outcome, RecoveryOutcome::Fresh);
        s.update_checkpoint("a", "boot-1", Some("step-1".into()))
            .await
            .unwrap();
        assert_eq!(
            s.get("a").await.unwrap().unwrap().checkpoint_id.as_deref(),
            Some("step-1")
        );
        assert!(s.get("missing").await.unwrap().is_none());
    }

    #[test]
    fn version_seven_store_migrates_terminal_settlement_outbox_on_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteTaskStore::new(dir.path()).unwrap();
        {
            let conn = store.conn.lock();
            conn.execute_batch(
                "DROP TABLE terminal_settlement_intents;
                 PRAGMA user_version = 7;",
            )
            .unwrap();
        }
        drop(store);

        let reopened = SqliteTaskStore::new(dir.path()).unwrap();
        let conn = reopened.conn.lock();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let outbox_exists: i64 = conn
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM sqlite_master
                     WHERE type = 'table' AND name = 'terminal_settlement_intents'
                )",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(version, CONTROL_PLANE_SCHEMA_VERSION);
        assert_eq!(outbox_exists, 1);
    }

    #[tokio::test]
    async fn update_status_sets_terminal_and_finished_at() {
        let s = SqliteTaskStore::new_in_memory().unwrap();
        s.create(rec("a", "main", 1, "boot-1")).await.unwrap();
        s.update_status("a", TaskStatus::Completed, Some("done".into()), None)
            .await
            .unwrap();
        let got = s.get("a").await.unwrap().unwrap();
        assert_eq!(got.status, TaskStatus::Completed);
        assert!(got.finished_at.is_some());
    }

    #[tokio::test]
    async fn terminal_transition_atomically_records_the_winning_outcome() {
        let s = SqliteTaskStore::new_in_memory().unwrap();
        s.create(rec("atomic", "main", 1, "boot-1")).await.unwrap();

        assert!(
            s.transition_terminal(
                "atomic",
                TaskStatus::Completed,
                Some("delegate_results/atomic.json".into()),
                None,
            )
            .await
            .unwrap()
        );

        let snapshot = s.get_snapshot("atomic").await.unwrap().unwrap();
        assert_eq!(snapshot.task.status, TaskStatus::Completed);
        assert_eq!(
            snapshot.output.as_deref(),
            Some("delegate_results/atomic.json")
        );
        assert!(snapshot.error.is_none());
        assert!(snapshot.task.finished_at.is_some());
    }

    #[tokio::test]
    async fn owner_checked_terminal_transition_requires_the_current_owner() {
        let s = SqliteTaskStore::new_in_memory().unwrap();
        s.create(rec("owner-match", "main", 7, "boot-old"))
            .await
            .unwrap();

        assert!(
            s.transition_terminal_if_owner(
                "owner-match",
                7,
                "boot-old",
                TaskStatus::Completed,
                Some("done".into()),
                None,
            )
            .await
            .unwrap()
        );
        assert_eq!(
            s.get("owner-match").await.unwrap().unwrap().status,
            TaskStatus::Completed
        );

        s.create(rec("owner-transfer", "main", 7, "boot-old"))
            .await
            .unwrap();
        s.claim_owner("owner-transfer", 42, "boot-new")
            .await
            .unwrap();

        assert!(
            !s.transition_terminal_if_owner(
                "owner-transfer",
                7,
                "boot-old",
                TaskStatus::Failed,
                None,
                Some("stale owner".into()),
            )
            .await
            .unwrap()
        );
        let transferred = s.get("owner-transfer").await.unwrap().unwrap();
        assert_eq!(transferred.status, TaskStatus::Running);
        assert_eq!(
            (transferred.owner_pid, transferred.owner_boot_id.as_str()),
            (42, "boot-new")
        );
        assert!(
            s.transition_terminal_if_owner(
                "owner-transfer",
                42,
                "boot-new",
                TaskStatus::Completed,
                Some("resumed".into()),
                None,
            )
            .await
            .unwrap()
        );
    }

    #[tokio::test]
    async fn competing_terminal_transition_cannot_overwrite_the_winner() {
        let s = SqliteTaskStore::new_in_memory().unwrap();
        s.create(rec("race", "main", 1, "boot-1")).await.unwrap();

        assert!(
            s.transition_terminal(
                "race",
                TaskStatus::Cancelled,
                None,
                Some("cancelled by user request".into()),
            )
            .await
            .unwrap()
        );
        assert!(
            !s.transition_terminal(
                "race",
                TaskStatus::Completed,
                Some("delegate_results/race.json".into()),
                None,
            )
            .await
            .unwrap()
        );

        let snapshot = s.get_snapshot("race").await.unwrap().unwrap();
        assert_eq!(snapshot.task.status, TaskStatus::Cancelled);
        assert!(snapshot.output.is_none());
        assert_eq!(snapshot.error.as_deref(), Some("cancelled by user request"));
    }

    #[tokio::test]
    async fn timeout_reconciliation_requires_the_observed_owner_and_heartbeat() {
        let s = SqliteTaskStore::new_in_memory().unwrap();
        let mut task = rec("timeout-race", "main", 7, "boot-1");
        task.heartbeat_at = Some("2026-06-18T00:00:00Z".into());
        s.create(task).await.unwrap();

        assert!(
            !s.reconcile_timed_out("timeout-race", 7, "boot-1", "2026-06-18T00:00:01Z",)
                .await
                .unwrap()
        );
        assert_eq!(
            s.get("timeout-race").await.unwrap().unwrap().status,
            TaskStatus::Running
        );
        assert!(
            s.reconcile_timed_out("timeout-race", 7, "boot-1", "2026-06-18T00:00:00Z",)
                .await
                .unwrap()
        );
        assert_eq!(
            s.get("timeout-race").await.unwrap().unwrap().status,
            TaskStatus::TimedOut
        );
        assert_eq!(
            s.get("timeout-race")
                .await
                .unwrap()
                .unwrap()
                .recovery_outcome,
            RecoveryOutcome::NeedsReview
        );
    }

    #[tokio::test]
    async fn list_running_and_by_agent() {
        let s = SqliteTaskStore::new_in_memory().unwrap();
        s.create(rec("a", "main", 1, "b")).await.unwrap();
        s.create(rec("b", "main", 1, "b")).await.unwrap();
        s.create(rec("c", "other", 1, "b")).await.unwrap();
        s.update_status("b", TaskStatus::Completed, None, None)
            .await
            .unwrap();
        assert_eq!(s.list_running().await.unwrap().len(), 2); // a + c
        assert_eq!(s.list_by_agent("main").await.unwrap().len(), 2); // a + b
        assert_eq!(s.count_by_agent("main").unwrap(), 2);
    }

    #[tokio::test]
    async fn cancellation_is_idempotent_and_cascades_to_owned_children() {
        let s = SqliteTaskStore::new_in_memory().unwrap();
        s.create(rec("parent", "main", 1, "boot-1")).await.unwrap();

        let mut child = rec("child", "main", 1, "boot-1");
        child.parent_id = Some("parent".into());
        s.create(child).await.unwrap();

        let mut grandchild = rec("grandchild", "main", 1, "boot-1");
        grandchild.parent_id = Some("child".into());
        s.create(grandchild).await.unwrap();

        let mut terminal_child = rec("terminal-child", "main", 1, "boot-1");
        terminal_child.parent_id = Some("parent".into());
        s.create(terminal_child).await.unwrap();
        s.update_status("terminal-child", TaskStatus::Completed, None, None)
            .await
            .unwrap();

        assert!(s.request_cancellation("parent").await.unwrap());
        assert!(!s.request_cancellation("parent").await.unwrap());
        s.cascade_cancellation("parent").await.unwrap();

        assert_eq!(
            s.get("parent").await.unwrap().unwrap().cancellation_state,
            CancellationState::Requested
        );
        for id in ["child", "grandchild"] {
            assert_eq!(
                s.get(id).await.unwrap().unwrap().cancellation_state,
                CancellationState::Requested
            );
        }
        let terminal = s.get("terminal-child").await.unwrap().unwrap();
        assert_eq!(terminal.status, TaskStatus::Completed);
        assert_eq!(terminal.cancellation_state, CancellationState::None);
    }

    #[tokio::test]
    async fn task_events_are_redacted_and_paginated() {
        let s = SqliteTaskStore::new_in_memory().unwrap();
        s.create(rec("events", "main", 1, "boot-1")).await.unwrap();
        s.record_task_event(
            "events",
            "tool_call",
            &serde_json::json!({
                "prompt": "private instruction",
                "nested": {"api_key": "secret-value"},
                "arguments": {"command": "cat private.txt"},
                "visible": "kept"
            }),
        )
        .await
        .unwrap();

        let events = s.list_task_events("events", 1, 0).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload["prompt"], "[REDACTED]");
        assert_eq!(events[0].payload["nested"]["api_key"], "[REDACTED]");
        assert_eq!(events[0].payload["arguments"], "[REDACTED]");
        assert_eq!(events[0].payload["visible"], "kept");
        assert!(s.list_task_events("events", 1, 1).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn reconcile_lost_only_when_authoritative() {
        let s = SqliteTaskStore::new_in_memory().unwrap();
        // prior-boot orphan ⇒ reclaimable
        s.create(rec("orphan", "main", 999_999, "boot-OLD"))
            .await
            .unwrap();
        assert!(s.reconcile_lost("orphan", "boot-NEW").await.unwrap());
        assert_eq!(
            s.get("orphan").await.unwrap().unwrap().status,
            TaskStatus::Lost
        );
        assert_eq!(
            s.get("orphan").await.unwrap().unwrap().recovery_outcome,
            RecoveryOutcome::Lost
        );

        // live same-boot owner ⇒ NOT reclaimable (split-brain guard)
        let me = std::process::id();
        s.create(rec("live", "main", me, "boot-NEW")).await.unwrap();
        assert!(!s.reconcile_lost("live", "boot-NEW").await.unwrap());
        assert_eq!(
            s.get("live").await.unwrap().unwrap().status,
            TaskStatus::Running
        );

        // already-terminal ⇒ no-op
        s.create(rec("done", "main", 0, "boot-OLD")).await.unwrap();
        s.update_status("done", TaskStatus::Completed, None, None)
            .await
            .unwrap();
        assert!(!s.reconcile_lost("done", "boot-NEW").await.unwrap());
    }

    #[tokio::test]
    async fn heartbeat_only_from_owner_boot() {
        let s = SqliteTaskStore::new_in_memory().unwrap();
        s.create(rec("a", "main", 1, "boot-1")).await.unwrap();
        s.heartbeat("a", "boot-OTHER").await.unwrap(); // wrong boot: no-op
        assert!(s.get("a").await.unwrap().unwrap().heartbeat_at.is_none());
        s.heartbeat("a", "boot-1").await.unwrap(); // owner: stamps
        assert!(s.get("a").await.unwrap().unwrap().heartbeat_at.is_some());
    }

    #[tokio::test]
    async fn claim_owner_updates_canonical_owner_fields_for_resumed_task() {
        let s = SqliteTaskStore::new_in_memory().unwrap();
        s.create(rec("a", "main", 1, "boot-old")).await.unwrap();

        s.claim_owner("a", 42, "boot-new").await.unwrap();

        let got = s.get("a").await.unwrap().unwrap();
        assert_eq!(got.owner_pid, 42);
        assert_eq!(got.owner_boot_id, "boot-new");
        assert_eq!(got.recovery_outcome, RecoveryOutcome::Resumed);
        assert!(got.heartbeat_at.is_none());
    }

    #[tokio::test]
    async fn claim_owner_removes_only_the_prior_owners_settlement_intent() {
        let s = SqliteTaskStore::new_in_memory().unwrap();
        s.create(rec("a", "main", 1, "boot-old")).await.unwrap();
        let intent = TerminalSettlementIntent {
            task_id: "a".into(),
            owner_pid: 1,
            owner_boot_id: "boot-old".into(),
            desired_status: TaskStatus::Completed,
            artifact_path: "/tmp/a.json".into(),
            artifact_ref: Some("artifact:a.json".into()),
            artifact_sha256: "00".repeat(32),
            terminal_error: None,
        };
        assert!(s.persist_terminal_settlement_intent(intent).await.unwrap());

        s.claim_owner("a", 1, "boot-old").await.unwrap();
        assert_eq!(s.list_terminal_settlement_intents().await.unwrap().len(), 1);

        s.claim_owner("a", 42, "boot-new").await.unwrap();

        assert!(
            s.list_terminal_settlement_intents()
                .await
                .unwrap()
                .is_empty()
        );
        let got = s.get("a").await.unwrap().unwrap();
        assert_eq!(
            (got.owner_pid, got.owner_boot_id.as_str()),
            (42, "boot-new")
        );
    }
}
