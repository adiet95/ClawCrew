import { useCallback, useEffect, useState } from 'react';
import { usePolling } from '@/hooks/usePolling';
import { Link } from 'react-router-dom';
import { Loader2, RefreshCw } from 'lucide-react';
import {
  apiFetch,
  cancelTask,
  getTaskStats,
  getTaskTimeline,
  getTaskTree,
  reopenTask,
  type TaskAgentStat,
  type TaskLedgerEntry,
  type TaskTreeRecord,
} from '@/lib/api';
import { apiOrigin, basePath } from '@/lib/basePath';
import { SSEClient } from '@/lib/sse';
import { formatRelative } from '@/lib/format';
import {
  MAX_TASK_EVENTS,
  buildTaskTree,
  isTaskEventFrame,
  mergeTaskEvents,
  type TaskEvent,
  type TaskTreeNode,
} from './taskBoard.logic';
import { Badge, Button, Card, PageHeader } from '@/components/ui';

interface TaskSummary {
  task_id: string;
  kind: string;
  owner_agent: string;
  status: string;
  created_at: string;
  updated_at: string;
  progress: number;
}

interface TaskBoardResponse {
  active_tasks: TaskSummary[];
  completed_tasks: TaskSummary[];
  failed_tasks: TaskSummary[];
  paused_tasks: TaskSummary[];
}

interface TaskDetailResponse {
  task: TaskSummary & { recovery_outcome?: string; checkpoint_id?: string | null };
  output?: string | null;
  error?: string | null;
}

interface TaskEventsResponse {
  events: TaskEvent[];
}

type TaskStatus = 'running' | 'paused' | 'completed' | 'failed' | 'cancelled' | 'lost' | 'timed_out';

function statusTone(status: string): 'neutral' | 'ok' | 'warn' | 'error' {
  switch (status.toLowerCase()) {
    case 'completed':
      return 'ok';
    case 'paused':
    case 'running':
      return 'warn';
    case 'failed':
    case 'cancelled':
    case 'lost':
    case 'timed_out':
      return 'error';
    default:
      return 'neutral';
  }
}

function statusLabel(status: string): string {
  return status.replace(/_/g, ' ');
}

export default function TaskBoard() {
  const [tasks, setTasks] = useState<TaskBoardResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [selectedTask, setSelectedTask] = useState<TaskDetailResponse | null>(null);
  const [selectedEvents, setSelectedEvents] = useState<TaskEvent[]>([]);
  const [tree, setTree] = useState<TaskTreeRecord[]>([]);
  const [timeline, setTimeline] = useState<TaskLedgerEntry[]>([]);
  const [stats, setStats] = useState<TaskAgentStat[]>([]);
  const [live, setLive] = useState(false);
  const [reloadKey, setReloadKey] = useState(0);
  const [actionBusy, setActionBusy] = useState(false);

  const fetchTasks = useCallback(async () => {
    setLoading(true);
    try {
      const [result, statsResult] = await Promise.all([
        apiFetch<TaskBoardResponse>('/api/dashboard/tasks'),
        getTaskStats().catch(() => ({ agents: [] as TaskAgentStat[] })),
      ]);
      setTasks(result);
      setStats(statsResult.agents);
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Unable to load tasks');
    } finally {
      setLoading(false);
    }
  }, []);

  usePolling(() => void fetchTasks(), 2_000);

  useEffect(() => {
    if (!selectedTaskId) {
      setSelectedTask(null);
      setSelectedEvents([]);
      setTree([]);
      setTimeline([]);
      setLive(false);
      return;
    }
    let current = true;
    // Seed the detail + bounded history with a REST read, then keep the
    // activity feed live over SSE (reconnect + bounded replay handled by the
    // client, dedupe by event id).
    apiFetch<TaskDetailResponse>(`/api/dashboard/tasks/${encodeURIComponent(selectedTaskId)}`)
      .then((detail) => {
        if (current) setSelectedTask(detail);
      })
      .catch((cause) => {
        if (current) setError(cause instanceof Error ? cause.message : 'Unable to load task details');
      });
    apiFetch<TaskEventsResponse>(
      `/api/dashboard/tasks/${encodeURIComponent(selectedTaskId)}/events?limit=${MAX_TASK_EVENTS}&offset=0`,
    )
      .then((events) => {
        if (current) setSelectedEvents((prev) => mergeTaskEvents(prev, events.events));
      })
      .catch(() => {
        /* The live stream below still delivers events. */
      });
    getTaskTree(selectedTaskId)
      .then((records) => {
        if (current) setTree(records);
      })
      .catch(() => {
        /* A task without descendants returns an empty tree. */
      });
    getTaskTimeline(selectedTaskId)
      .then((response) => {
        if (current) setTimeline(response.entries);
      })
      .catch(() => {
        /* A task without a work ledger returns an empty timeline. */
      });

    const client = new SSEClient({
      path: `${apiOrigin}${basePath}/api/dashboard/tasks/${encodeURIComponent(selectedTaskId)}/stream`,
    });
    client.onConnect = () => current && setLive(true);
    client.onError = () => current && setLive(false);
    client.onEvent = (frame) => {
      if (!current || !isTaskEventFrame(frame)) return;
      const event = frame as unknown as TaskEvent;
      if (typeof event.id !== 'number') return;
      setSelectedEvents((prev) => mergeTaskEvents(prev, [event]));
    };
    client.connect();

    return () => {
      current = false;
      client.disconnect();
      setLive(false);
    };
  }, [selectedTaskId, reloadKey]);

  const reloadSelected = useCallback(() => {
    setReloadKey((key) => key + 1);
    void fetchTasks();
  }, [fetchTasks]);

  const runAction = useCallback(
    (action: Promise<unknown>, label: string) => {
      setActionBusy(true);
      action
        .then(() => reloadSelected())
        .catch((cause) =>
          setError(cause instanceof Error ? cause.message : `${label} failed`),
        )
        .finally(() => setActionBusy(false));
    },
    [reloadSelected],
  );

  return (
    <div className="space-y-4">
      <PageHeader
        title="Task Board"
        description="Durable background work and recovery state."
        actions={
          <Button variant="ghost" size="sm" onClick={() => void fetchTasks()} disabled={loading}>
            {loading ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden /> : <RefreshCw className="h-4 w-4" aria-hidden />}
            Refresh
          </Button>
        }
      />

      {error ? (
        <Card className="border-status-error/30 text-sm text-status-error">
          <p>Task data is unavailable: {error}</p>
          <p className="mt-1 text-xs text-pc-text-muted">
            The dashboard task endpoint must be wired before this view can display live work.
          </p>
        </Card>
      ) : loading && !tasks ? (
        <div className="flex items-center justify-center py-16 text-pc-text-muted">
          <Loader2 className="mr-2 h-5 w-5 animate-spin" aria-hidden />
          Loading tasks...
        </div>
      ) : tasks ? (
        <>
          <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
            <TaskColumn title="Active" tasks={tasks.active_tasks} onSelect={setSelectedTaskId} />
            <TaskColumn title="Paused" tasks={tasks.paused_tasks} onSelect={setSelectedTaskId} />
            <TaskColumn title="Completed" tasks={tasks.completed_tasks} onSelect={setSelectedTaskId} />
            <TaskColumn title="Failed" tasks={tasks.failed_tasks} onSelect={setSelectedTaskId} />
          </div>
          {stats.length > 0 ? <AgentPerformance stats={stats} /> : null}
          {selectedTask ? (
            <TaskDetailPanel
              detail={selectedTask}
              events={selectedEvents}
              live={live}
              tree={tree}
              timeline={timeline}
              busy={actionBusy}
              onSelect={setSelectedTaskId}
              onCancel={(id) => runAction(cancelTask(id), 'Cancel')}
              onReopen={(id) => runAction(reopenTask(id), 'Reopen')}
            />
          ) : null}
        </>
      ) : null}
    </div>
  );
}

function TaskColumn({ title, tasks, onSelect }: { title: string; tasks: TaskSummary[]; onSelect: (taskId: string) => void }) {
  return (
    <Card className="min-w-0">
      <div className="mb-4 flex items-center justify-between gap-2">
        <h2 className="text-sm font-semibold text-pc-text">{title}</h2>
        <Badge>{tasks.length}</Badge>
      </div>
      <div className="space-y-3">
        {tasks.map((task) => {
          const progress = Math.min(1, Math.max(0, task.progress));
          const status = task.status as TaskStatus;
          return (
            <article
              key={task.task_id}
              className="cursor-pointer rounded border border-pc-border bg-pc-elevated p-3 hover:border-pc-accent"
              onClick={() => onSelect(task.task_id)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' || event.key === ' ') onSelect(task.task_id);
              }}
              role="button"
              tabIndex={0}
            >
              <div className="flex items-start justify-between gap-3">
                <code className="min-w-0 truncate text-xs text-pc-text" title={task.task_id}>
                  {task.task_id}
                </code>
                <Badge tone={statusTone(status)}>{statusLabel(status)}</Badge>
              </div>
              <p className="mt-2 text-xs text-pc-text-secondary">Agent: {task.owner_agent}</p>
              <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-pc-border" aria-label={`${Math.round(progress * 100)}% complete`}>
                <div className="h-full rounded-full bg-pc-accent" style={{ width: `${progress * 100}%` }} />
              </div>
              <p className="mt-2 text-[11px] text-pc-text-muted">
                Updated {formatRelative(task.updated_at || task.created_at)}
              </p>
            </article>
          );
        })}
        {tasks.length === 0 ? <p className="text-xs italic text-pc-text-muted">No tasks</p> : null}
      </div>
    </Card>
  );
}

function AgentPerformance({ stats }: { stats: TaskAgentStat[] }) {
  const rows = [...stats].sort((a, b) => b.total - a.total);
  return (
    <Card className="overflow-hidden p-0">
      <div className="border-b border-pc-border px-4 py-2.5">
        <h2 className="text-sm font-semibold text-pc-text">Agent performance</h2>
        <p className="text-[11px] text-pc-text-muted">Task outcomes and average duration per agent.</p>
      </div>
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-pc-border text-left text-xs uppercase tracking-wide text-pc-text-muted">
            <th className="px-4 py-2 font-medium">Agent</th>
            <th className="px-4 py-2 font-medium">Total</th>
            <th className="px-4 py-2 font-medium">Active</th>
            <th className="px-4 py-2 font-medium">Completed</th>
            <th className="px-4 py-2 font-medium">Failed</th>
            <th className="px-4 py-2 font-medium">Avg duration</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-pc-border">
          {rows.map((stat) => (
            <tr key={stat.agent} className="hover:bg-pc-elevated/50">
              <td className="px-4 py-2 font-medium">
                <Link to={`/agent/${encodeURIComponent(stat.agent)}`} className="text-pc-accent hover:underline">
                  {stat.agent}
                </Link>
              </td>
              <td className="px-4 py-2 tabular-nums text-pc-text-secondary">{stat.total}</td>
              <td className="px-4 py-2 tabular-nums text-pc-text-secondary">{stat.active}</td>
              <td className="px-4 py-2 tabular-nums text-status-success">{stat.completed}</td>
              <td className="px-4 py-2 tabular-nums text-status-error">{stat.failed}</td>
              <td className="px-4 py-2 tabular-nums text-pc-text-muted">
                {stat.avg_duration_ms != null ? `${(stat.avg_duration_ms / 1000).toFixed(1)}s` : '—'}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </Card>
  );
}

function TaskDetailPanel({
  detail,
  events,
  live,
  tree,
  timeline,
  busy,
  onSelect,
  onCancel,
  onReopen,
}: {
  detail: TaskDetailResponse;
  events: TaskEvent[];
  live: boolean;
  tree: TaskTreeRecord[];
  timeline: TaskLedgerEntry[];
  busy: boolean;
  onSelect: (taskId: string) => void;
  onCancel: (taskId: string) => void;
  onReopen: (taskId: string) => void;
}) {
  const taskStatus = detail.task.status;
  const canCancel = !['completed', 'cancelled', 'lost', 'timed_out'].includes(taskStatus);
  const canReopen = ['paused', 'failed', 'needs_review'].includes(taskStatus);
  const reopenLabel =
    taskStatus === 'failed' ? 'Retry' : taskStatus === 'needs_review' ? 'Review' : 'Resume';
  return (
    <Card>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h2 className="text-sm font-semibold text-pc-text">Task detail</h2>
          <code className="text-xs text-pc-text-muted">{detail.task.task_id}</code>
        </div>
        <div className="flex items-center gap-2">
          <span className="inline-flex items-center gap-1 text-xs text-pc-text-muted">
            <span
              className={`h-2 w-2 rounded-full ${live ? 'bg-status-success' : 'bg-pc-text-muted'}`}
              aria-hidden
            />
            {live ? 'Live' : 'Reconnecting…'}
          </span>
          <Badge tone={statusTone(detail.task.status)}>{statusLabel(detail.task.status)}</Badge>
          {canReopen ? (
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => onReopen(detail.task.task_id)}>
              {reopenLabel}
            </Button>
          ) : null}
          {canCancel ? (
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => onCancel(detail.task.task_id)}>
              Cancel
            </Button>
          ) : null}
        </div>
      </div>
      <div className="mt-4 grid gap-3 text-xs text-pc-text-secondary md:grid-cols-3">
        <span>
          Agent:{' '}
          <Link
            to={`/agent/${encodeURIComponent(detail.task.owner_agent)}`}
            className="text-pc-accent hover:underline"
          >
            {detail.task.owner_agent}
          </Link>
        </span>
        <span>
          <Link
            to={`/agent/${encodeURIComponent(detail.task.owner_agent)}/workspace`}
            className="text-pc-accent hover:underline"
          >
            Workspace
          </Link>
        </span>
        <span>Recovery: {detail.task.recovery_outcome ?? 'fresh'}</span>
        <span>Checkpoint: {detail.task.checkpoint_id ?? 'none'}</span>
      </div>
      {tree.length > 0 ? (
        <div className="mt-4 border-t border-pc-border pt-3">
          <h3 className="text-xs font-semibold uppercase tracking-wide text-pc-text-muted">
            Subagents ({tree.length})
          </h3>
          <div className="mt-2 space-y-0.5">
            <TaskTreeBranch
              nodes={buildTaskTree(detail.task.task_id, tree)}
              onSelect={onSelect}
              depth={0}
            />
          </div>
        </div>
      ) : null}
      {timeline.length > 0 ? (
        <div className="mt-4 border-t border-pc-border pt-3">
          <h3 className="text-xs font-semibold uppercase tracking-wide text-pc-text-muted">
            Checkpoint timeline
          </h3>
          <ol className="mt-2 space-y-1">
            {timeline.map((entry, index) => (
              <li
                key={`${entry.timestamp}-${entry.step_id}-${entry.action}-${index}`}
                className="flex items-start gap-2 text-xs"
              >
                <span className="mt-1 h-1.5 w-1.5 shrink-0 rounded-full bg-pc-accent" aria-hidden />
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2 text-pc-text-secondary">
                    <Badge>{entry.action.replace(/_/g, ' ')}</Badge>
                    <code className="text-[11px] text-pc-text-muted">{entry.step_id}</code>
                    <span className="text-[11px] text-pc-text-muted">
                      {formatRelative(entry.timestamp)}
                    </span>
                  </div>
                  {entry.details ? (
                    <pre className="mt-1 max-h-24 overflow-auto whitespace-pre-wrap text-[11px] text-pc-text-muted">
                      {entry.details}
                    </pre>
                  ) : null}
                </div>
              </li>
            ))}
          </ol>
        </div>
      ) : null}
      <div className="mt-4 border-t border-pc-border pt-3">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-pc-text-muted">Activity</h3>
        <div className="mt-2 space-y-2">
          {events.map((event) => (
            <div key={event.id} className="rounded border border-pc-border bg-pc-elevated p-2 text-xs">
              <div className="flex justify-between gap-2 text-pc-text-secondary">
                <span>{event.event_type}</span>
                <span>{formatRelative(event.timestamp)}</span>
              </div>
              <pre className="mt-1 max-h-24 overflow-auto whitespace-pre-wrap text-[11px] text-pc-text-muted">{JSON.stringify(event.payload)}</pre>
            </div>
          ))}
          {events.length === 0 ? <p className="text-xs italic text-pc-text-muted">No recorded events</p> : null}
        </div>
      </div>
    </Card>
  );
}

function TaskTreeBranch({
  nodes,
  onSelect,
  depth,
}: {
  nodes: TaskTreeNode<TaskTreeRecord>[];
  onSelect: (taskId: string) => void;
  depth: number;
}) {
  return (
    <>
      {nodes.map((node) => (
        <div key={node.record.id}>
          <button
            type="button"
            className="flex w-full items-center justify-between gap-2 rounded px-2 py-1 text-left text-xs hover:bg-pc-elevated"
            style={{ paddingLeft: `${depth * 14 + 8}px` }}
            onClick={() => onSelect(node.record.id)}
          >
            <code className="min-w-0 truncate text-pc-text" title={node.record.id}>
              {node.record.id}
            </code>
            <Badge tone={statusTone(node.record.status)}>{statusLabel(node.record.status)}</Badge>
          </button>
          {node.children.length > 0 ? (
            <TaskTreeBranch nodes={node.children} onSelect={onSelect} depth={depth + 1} />
          ) : null}
        </div>
      ))}
    </>
  );
}

