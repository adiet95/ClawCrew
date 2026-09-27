//! Authenticated projections for the durable control plane.

use super::{AppState, api::require_auth};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Json, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{convert::Infallible, time::Duration};

const MAX_PAGE_SIZE: u32 = 100;
const MAX_EVENT_PAYLOAD_BYTES: usize = 16 * 1024;

#[derive(Debug, Deserialize)]
pub struct PageQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

impl PageQuery {
    fn bounds(&self) -> (u32, u32) {
        (
            self.limit.unwrap_or(50).clamp(1, MAX_PAGE_SIZE),
            self.offset.unwrap_or(0),
        )
    }
}

#[derive(Debug, Serialize)]
struct TaskSummary {
    task_id: String,
    kind: String,
    owner_agent: String,
    status: String,
    created_at: String,
    updated_at: String,
    progress: f32,
    /// Wall-clock duration in milliseconds for a finished task, else `null`.
    duration_ms: Option<i64>,
}

/// Per-agent aggregate of durable task outcomes and duration.
#[derive(Debug, Serialize, PartialEq, Eq)]
struct AgentStat {
    agent: String,
    total: usize,
    active: usize,
    completed: usize,
    failed: usize,
    avg_duration_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
struct TaskBoardResponse {
    active_tasks: Vec<TaskSummary>,
    paused_tasks: Vec<TaskSummary>,
    completed_tasks: Vec<TaskSummary>,
    failed_tasks: Vec<TaskSummary>,
    total: usize,
}

#[derive(Debug, Serialize)]
struct TaskDetail {
    #[serde(flatten)]
    task: zeroclaw_runtime::control_plane::TaskRecord,
    output: Option<String>,
    error: Option<String>,
}

fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({
            "error": "control plane is not initialized"
        })),
    )
        .into_response()
}

fn internal(error: impl std::fmt::Display) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "error": format!("control plane query failed: {error}")
        })),
    )
        .into_response()
}

fn summary(task: zeroclaw_runtime::control_plane::TaskRecord) -> TaskSummary {
    let status = serde_json::to_value(task.status).unwrap_or(Value::String("unknown".into()));
    let status = status.as_str().unwrap_or("unknown").to_string();
    let progress = (status == "completed") as u8 as f32;
    let duration_ms = task_duration_ms(&task.started_at, task.finished_at.as_deref());
    TaskSummary {
        task_id: task.id,
        kind: serde_json::to_value(task.kind)
            .unwrap_or(Value::String("unknown".into()))
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        owner_agent: task.agent,
        status,
        created_at: task.started_at.clone(),
        updated_at: task.finished_at.unwrap_or(task.started_at),
        progress,
        duration_ms,
    }
}

fn task_duration_ms(started: &str, finished: Option<&str>) -> Option<i64> {
    let start = chrono::DateTime::parse_from_rfc3339(started).ok()?;
    let end = chrono::DateTime::parse_from_rfc3339(finished?).ok()?;
    Some((end - start).num_milliseconds().max(0))
}

/// Aggregate durable task outcomes and average duration per owning agent.
fn aggregate_agent_stats(tasks: &[zeroclaw_runtime::control_plane::TaskRecord]) -> Vec<AgentStat> {
    use zeroclaw_runtime::control_plane::TaskStatus;
    let mut map: std::collections::BTreeMap<String, (usize, usize, usize, usize, i64, usize)> =
        std::collections::BTreeMap::new();
    for task in tasks {
        let entry = map.entry(task.agent.clone()).or_default();
        entry.0 += 1;
        match task.status {
            TaskStatus::Completed => entry.2 += 1,
            TaskStatus::Failed
            | TaskStatus::Cancelled
            | TaskStatus::Lost
            | TaskStatus::TimedOut => entry.3 += 1,
            _ => entry.1 += 1,
        }
        if let Some(ms) = task_duration_ms(&task.started_at, task.finished_at.as_deref()) {
            entry.4 += ms;
            entry.5 += 1;
        }
    }
    map.into_iter()
        .map(
            |(agent, (total, active, completed, failed, total_ms, measured))| AgentStat {
                agent,
                total,
                active,
                completed,
                failed,
                avg_duration_ms: (measured > 0).then(|| total_ms / measured as i64),
            },
        )
        .collect()
}

pub async fn handle_tasks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PageQuery>,
) -> Response {
    if let Err(response) = require_auth(&state, &headers) {
        return response.into_response();
    }
    let Some(control_plane) = zeroclaw_runtime::control_plane::control_plane() else {
        return unavailable();
    };
    let (limit, offset) = query.bounds();
    let tasks = match control_plane.store.list_all().await {
        Ok(tasks) => tasks,
        Err(error) => return internal(error),
    };
    let page = tasks.into_iter().skip(offset as usize).take(limit as usize);
    let mut response = TaskBoardResponse {
        active_tasks: Vec::new(),
        paused_tasks: Vec::new(),
        completed_tasks: Vec::new(),
        failed_tasks: Vec::new(),
        total: 0,
    };
    for task in page {
        response.total += 1;
        match task.status {
            zeroclaw_runtime::control_plane::TaskStatus::Queued
            | zeroclaw_runtime::control_plane::TaskStatus::Waiting
            | zeroclaw_runtime::control_plane::TaskStatus::Validating
            | zeroclaw_runtime::control_plane::TaskStatus::Retrying
            | zeroclaw_runtime::control_plane::TaskStatus::Running => {
                response.active_tasks.push(summary(task))
            }
            zeroclaw_runtime::control_plane::TaskStatus::Paused => {
                response.paused_tasks.push(summary(task))
            }
            zeroclaw_runtime::control_plane::TaskStatus::Completed => {
                response.completed_tasks.push(summary(task))
            }
            zeroclaw_runtime::control_plane::TaskStatus::NeedsReview => {
                response.paused_tasks.push(summary(task))
            }
            _ => response.failed_tasks.push(summary(task)),
        }
    }
    Json(response).into_response()
}

pub async fn handle_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Response {
    if let Err(response) = require_auth(&state, &headers) {
        return response.into_response();
    }
    let Some(control_plane) = zeroclaw_runtime::control_plane::control_plane() else {
        return unavailable();
    };
    match control_plane.store.get_snapshot(&task_id).await {
        Ok(Some(snapshot)) => Json(TaskDetail {
            task: snapshot.task,
            output: snapshot.output,
            error: snapshot.error,
        })
        .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal(error),
    }
}

pub async fn handle_task_children(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Response {
    if let Err(response) = require_auth(&state, &headers) {
        return response.into_response();
    }
    let Some(control_plane) = zeroclaw_runtime::control_plane::control_plane() else {
        return unavailable();
    };
    match control_plane.store.get_child_tasks(&task_id).await {
        Ok(children) => Json(children).into_response(),
        Err(error) => internal(error),
    }
}

/// GET /api/dashboard/tasks/:id/tree — every descendant of a root task,
/// recursively, as the durable parent-child subtree projection.
pub async fn handle_task_tree(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Response {
    if let Err(response) = require_auth(&state, &headers) {
        return response.into_response();
    }
    let Some(control_plane) = zeroclaw_runtime::control_plane::control_plane() else {
        return unavailable();
    };
    match control_plane.store.list_descendants(&task_id).await {
        Ok(descendants) => Json(descendants).into_response(),
        Err(error) => internal(error),
    }
}

/// GET /api/dashboard/tasks/:id/timeline — the ordered TaskRunner work ledger
/// (checkpoint/step timeline).
pub async fn handle_task_timeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Response {
    if let Err(response) = require_auth(&state, &headers) {
        return response.into_response();
    }
    let Some(control_plane) = zeroclaw_runtime::control_plane::control_plane() else {
        return unavailable();
    };
    match control_plane.store.list_task_ledger(&task_id).await {
        Ok(entries) => Json(serde_json::json!({ "entries": entries })).into_response(),
        Err(error) => internal(error),
    }
}

/// POST /api/dashboard/tasks/:id/cancel — request cancellation of a task and
/// cascade it to owned descendants.
pub async fn handle_task_cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Response {
    if let Err(response) = require_auth(&state, &headers) {
        return response.into_response();
    }
    let Some(control_plane) = zeroclaw_runtime::control_plane::control_plane() else {
        return unavailable();
    };
    let requested = match control_plane.store.request_cancellation(&task_id).await {
        Ok(requested) => requested,
        Err(error) => return internal(error),
    };
    if let Err(error) = control_plane.store.cascade_cancellation(&task_id).await {
        return internal(error);
    }
    Json(serde_json::json!({ "cancelled": requested })).into_response()
}

/// POST /api/dashboard/tasks/:id/reopen — operator resume/retry/review: reopen a
/// `paused`, `needs_review`, or `failed` task back to `running`.
pub async fn handle_task_reopen(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Response {
    if let Err(response) = require_auth(&state, &headers) {
        return response.into_response();
    }
    let Some(control_plane) = zeroclaw_runtime::control_plane::control_plane() else {
        return unavailable();
    };
    match control_plane.store.reopen_for_operator(&task_id).await {
        Ok(reopened) => Json(serde_json::json!({ "reopened": reopened })).into_response(),
        Err(error) => internal(error),
    }
}

/// GET /api/dashboard/tasks/stats — per-agent task outcome + duration rollup.
pub async fn handle_task_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = require_auth(&state, &headers) {
        return response.into_response();
    }
    let Some(control_plane) = zeroclaw_runtime::control_plane::control_plane() else {
        return unavailable();
    };
    match control_plane.store.list_all().await {
        Ok(tasks) => {
            Json(serde_json::json!({ "agents": aggregate_agent_stats(&tasks) })).into_response()
        }
        Err(error) => internal(error),
    }
}

pub async fn handle_task_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Response {
    if let Err(response) = require_auth(&state, &headers) {
        return response.into_response();
    }
    let Some(control_plane) = zeroclaw_runtime::control_plane::control_plane() else {
        return unavailable();
    };
    let (limit, offset) = query.bounds();
    let events = match control_plane
        .store
        .list_task_events(&task_id, limit, offset)
        .await
    {
        Ok(events) => events,
        Err(error) => return internal(error),
    };
    let events = events
        .into_iter()
        .map(|mut event| {
            event.payload = redact_event_payload(event.payload);
            event
        })
        .collect::<Vec<_>>();
    Json(serde_json::json!({ "events": events })).into_response()
}

#[derive(Debug, Deserialize)]
pub struct EventStreamQuery {
    pub since: Option<i64>,
}

/// GET /api/dashboard/tasks/:id/stream — authenticated bounded task event stream.
pub async fn handle_task_events_stream(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    Query(query): Query<EventStreamQuery>,
) -> Response {
    if let Err(response) = require_auth(&state, &headers) {
        return response.into_response();
    }
    let Some(control_plane) = zeroclaw_runtime::control_plane::control_plane() else {
        return unavailable();
    };
    let store = std::sync::Arc::clone(&control_plane.store);
    let since = query.since.unwrap_or(0);
    let stream = futures_util::stream::unfold(
        (store, task_id, since, std::collections::VecDeque::new()),
        |(store, task_id, mut last_id, mut queue)| async move {
            if queue.is_empty() {
                tokio::time::sleep(Duration::from_secs(1)).await;
                match store.list_task_events(&task_id, 100, 0).await {
                    Ok(events) => {
                        let mut new_events = events
                            .into_iter()
                            .filter(|event| event.id > last_id)
                            .collect::<Vec<_>>();
                        new_events.sort_by_key(|event| event.id);
                        queue.extend(new_events);
                    }
                    Err(error) => {
                        let error_event = Event::default()
                            .event("error")
                            .data(serde_json::json!({"error": error.to_string()}).to_string());
                        return Some((
                            Ok::<Event, Infallible>(error_event),
                            (store, task_id, last_id, queue),
                        ));
                    }
                }
            }

            if let Some(event) = queue.pop_front() {
                last_id = std::cmp::max(last_id, event.id);
                // Emit the full event so a live consumer renders the same shape
                // as the REST `/events` projection (id + type + timestamp +
                // redacted payload) and can dedupe by `id` on reconnect.
                let task_event = serde_json::json!({
                    "id": event.id,
                    "task_id": event.task_id,
                    "event_type": event.event_type,
                    "payload": redact_event_payload(event.payload),
                    "timestamp": event.timestamp,
                });
                let output = Event::default()
                    .event("task_event")
                    .id(event.id.to_string())
                    .data(serde_json::to_string(&task_event).unwrap_or_default());
                Some((
                    Ok::<Event, Infallible>(output),
                    (store, task_id, last_id, queue),
                ))
            } else {
                let output = Event::default().comment("no new task events");
                Some((
                    Ok::<Event, Infallible>(output),
                    (store, task_id, last_id, queue),
                ))
            }
        },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn redact_event_payload(value: Value) -> Value {
    let redacted = redact_value(value);
    let encoded = serde_json::to_vec(&redacted).unwrap_or_default();
    if encoded.len() <= MAX_EVENT_PAYLOAD_BYTES {
        return redacted;
    }
    serde_json::json!({"redacted": true, "reason": "event payload exceeds projection limit"})
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
                        "prompt",
                        "credential",
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

#[cfg(test)]
mod tests {
    use super::*;
    use zeroclaw_runtime::control_plane::task_registry::CancellationState;
    use zeroclaw_runtime::control_plane::{TaskKind, TaskRecord, TaskStatus};

    fn rec(id: &str, agent: &str, status: TaskStatus, finished: Option<&str>) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            kind: TaskKind::Delegate,
            agent: agent.into(),
            status,
            owner_pid: 1,
            owner_boot_id: "boot".into(),
            heartbeat_at: None,
            depth: 0,
            parent_id: None,
            originator_route: None,
            delivered: false,
            idem_key: None,
            principal_id: None,
            session_key: None,
            workspace: None,
            cancellation_state: CancellationState::None,
            checkpoint_id: None,
            recovery_outcome: Default::default(),
            started_at: "2026-01-01T00:00:00Z".into(),
            finished_at: finished.map(str::to_string),
        }
    }

    #[test]
    fn aggregate_counts_outcomes_and_averages_duration_per_agent() {
        let tasks = vec![
            rec("a", "main", TaskStatus::Completed, Some("2026-01-01T00:00:02Z")),
            rec("b", "main", TaskStatus::Failed, Some("2026-01-01T00:00:04Z")),
            rec("c", "main", TaskStatus::Running, None),
            rec("d", "other", TaskStatus::Completed, Some("2026-01-01T00:00:01Z")),
        ];
        let stats = aggregate_agent_stats(&tasks);

        let main = stats.iter().find(|s| s.agent == "main").unwrap();
        assert_eq!(main.total, 3);
        assert_eq!(main.completed, 1);
        assert_eq!(main.failed, 1);
        assert_eq!(main.active, 1);
        assert_eq!(main.avg_duration_ms, Some(3000));

        let other = stats.iter().find(|s| s.agent == "other").unwrap();
        assert_eq!(other.total, 1);
        assert_eq!(other.avg_duration_ms, Some(1000));
    }

    #[test]
    fn task_duration_is_none_until_finished() {
        assert_eq!(task_duration_ms("2026-01-01T00:00:00Z", None), None);
        assert_eq!(
            task_duration_ms("2026-01-01T00:00:00Z", Some("2026-01-01T00:00:05Z")),
            Some(5000)
        );
    }
}
