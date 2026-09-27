//! SQLite bindings for the TaskRunner spec and ledger.

use rusqlite::{OptionalExtension, params};

use crate::control_plane::task_runner::{LedgerEntry, TaskRunnerSpec};

use super::SqliteTaskStore;

impl SqliteTaskStore {
    /// Save or overwrite the complete TaskRunnerSpec.
    pub fn save_task_runner_spec(&self, spec: &TaskRunnerSpec) -> Result<(), anyhow::Error> {
        let conn = self.conn.lock();
        let payload = serde_json::to_string(spec)?;

        conn.execute(
            "INSERT INTO task_runner_specs (task_id, spec_payload)
             VALUES (?1, ?2)
             ON CONFLICT(task_id) DO UPDATE SET spec_payload=excluded.spec_payload",
            params![spec.task_id, payload],
        )?;

        Ok(())
    }

    /// Load the TaskRunnerSpec by task id.
    pub fn get_task_runner_spec(
        &self,
        task_id: &str,
    ) -> Result<Option<TaskRunnerSpec>, anyhow::Error> {
        let conn = self.conn.lock();
        let payload: Option<String> = conn
            .query_row(
                "SELECT spec_payload FROM task_runner_specs WHERE task_id = ?1",
                params![task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        match payload {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    /// Append a new ledger entry. Duplicates are prevented by a unique constraint on (task_id, step_id, action, timestamp).
    pub fn append_ledger_entry(&self, entry: &LedgerEntry) -> Result<(), anyhow::Error> {
        let conn = self.conn.lock();
        let action = serde_json::to_value(&entry.action)?
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        conn.execute(
            "INSERT OR IGNORE INTO task_runner_ledger
                (task_id, step_id, timestamp, action, details, correlation_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                entry.task_id,
                entry.step_id,
                entry.timestamp,
                action,
                entry.details,
                entry.correlation_id,
            ],
        )?;

        Ok(())
    }

    /// Get all ledger entries for a specific task runner, sorted by timestamp.
    pub fn get_ledger_entries(&self, task_id: &str) -> Result<Vec<LedgerEntry>, anyhow::Error> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT step_id, timestamp, action, details, correlation_id
             FROM task_runner_ledger WHERE task_id = ?1 ORDER BY timestamp ASC",
        )?;

        let mut rows = stmt.query(params![task_id])?;
        let mut entries = Vec::new();

        while let Some(row) = rows.next()? {
            let action_str: String = row.get(2)?;
            let action = serde_json::from_value(serde_json::Value::String(action_str))
                .unwrap_or(crate::control_plane::task_runner::LedgerAction::Started);

            entries.push(LedgerEntry {
                task_id: task_id.to_string(),
                step_id: row.get::<_, String>(0)?,
                timestamp: row.get(1)?,
                action,
                details: row.get(3)?,
                correlation_id: row.get(4).unwrap_or(None),
            });
        }

        Ok(entries)
    }
}
