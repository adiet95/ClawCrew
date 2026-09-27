import { useCallback, useState } from 'react';
import { Link } from 'react-router-dom';
import { Loader2, RefreshCw } from 'lucide-react';
import {
  getRecoveryTasks,
  reopenTask,
  cancelTask,
  acknowledgeTask,
  type RecoveryTask,
} from '@/lib/api';
import { usePolling } from '@/hooks/usePolling';
import { formatRelative } from '@/lib/format';
import {
  RECOVERY_STATUSES,
  recoveryTone,
  recoveryLabel,
  canRetry,
  canAcknowledge,
  filterByStatus,
} from './recovery.logic';
import { Badge, Button, Card, ConfirmDialog, PageHeader, StatCard } from '@/components/ui';

export default function Recovery() {
  const [tasks, setTasks] = useState<RecoveryTask[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [statusFilter, setStatusFilter] = useState<string | null>(null);
  const [actionBusy, setActionBusy] = useState<string | null>(null);
  const [cancelTarget, setCancelTarget] = useState<string | null>(null);

  const fetchTasks = useCallback(async (isStale: () => boolean) => {
    try {
      const result = await getRecoveryTasks();
      if (!isStale()) {
        setTasks(result.tasks);
        setError(null);
      }
    } catch (cause) {
      if (!isStale()) setError(cause instanceof Error ? cause.message : 'Unable to load recovery tasks');
    } finally {
      if (!isStale()) setLoading(false);
    }
  }, []);

  usePolling(fetchTasks, 3_000);

  const runAction = useCallback(
    (taskId: string, action: Promise<unknown>, label: string) => {
      setActionBusy(taskId);
      action
        .then(() => void fetchTasks(() => false))
        .catch((cause) =>
          setError(cause instanceof Error ? cause.message : `${label} failed`),
        )
        .finally(() => setActionBusy(null));
    },
    [fetchTasks],
  );

  const handleCancel = useCallback(() => {
    if (!cancelTarget) return;
    runAction(cancelTarget, cancelTask(cancelTarget), 'Cancel');
    setCancelTarget(null);
  }, [cancelTarget, runAction]);

  const filtered = filterByStatus(tasks, statusFilter);
  const statusCounts = RECOVERY_STATUSES.map((s) => ({
    status: s,
    count: tasks.filter((t) => t.status === s).length,
  }));

  return (
    <div className="space-y-4">
      <PageHeader
        title="Recovery Console"
        description="Unified view of tasks needing operator attention — lost, stuck, or failed."
        actions={
          <Button variant="ghost" size="sm" onClick={() => void fetchTasks(() => false)} disabled={loading}>
            {loading ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden /> : <RefreshCw className="h-4 w-4" aria-hidden />}
            Refresh
          </Button>
        }
      />

      {error ? (
        <Card className="border-status-error/30 text-sm text-status-error">
          <p>Recovery data is unavailable: {error}</p>
          <p className="mt-1 text-xs text-pc-text-muted">
            The recovery endpoint must be wired before this view can display tasks needing attention.
          </p>
        </Card>
      ) : loading && tasks.length === 0 ? (
        <div className="flex items-center justify-center py-16 text-pc-text-muted">
          <Loader2 className="mr-2 h-5 w-5 animate-spin" aria-hidden />
          Loading recovery tasks…
        </div>
      ) : tasks.length === 0 ? (
        <Card className="p-6 text-center text-sm" style={{ color: 'var(--pc-text-faint)' }}>
          <p>No tasks need attention. All clear.</p>
        </Card>
      ) : (
        <>
          <div className="grid gap-4 md:grid-cols-5">
            <StatCard
              label="Needs attention"
              value={tasks.length}
              tone={tasks.length > 0 ? 'warn' : 'ok'}
            />
            {statusCounts.map(({ status, count }) => (
              <StatCard
                key={status}
                label={recoveryLabel(status)}
                value={count}
                tone={count > 0 ? (recoveryTone(status) === 'error' ? 'error' : 'warn') : 'ok'}
              />
            ))}
          </div>

          {/* Status filter tabs */}
          <div className="flex items-center gap-2 flex-wrap">
            <button
              type="button"
              onClick={() => setStatusFilter(null)}
              className={[
                'px-3 py-1.5 rounded-full text-xs font-medium border transition-colors',
                statusFilter === null
                  ? 'bg-pc-accent/10 text-pc-accent border-pc-accent/30'
                  : 'bg-pc-elevated text-pc-text-muted border-pc-border hover:text-pc-text',
              ].join(' ')}
            >
              All ({tasks.length})
            </button>
            {RECOVERY_STATUSES.map((s) => {
              const count = tasks.filter((t) => t.status === s).length;
              return (
                <button
                  key={s}
                  type="button"
                  onClick={() => setStatusFilter(s)}
                  className={[
                    'px-3 py-1.5 rounded-full text-xs font-medium border transition-colors',
                    statusFilter === s
                      ? 'bg-pc-accent/10 text-pc-accent border-pc-accent/30'
                      : 'bg-pc-elevated text-pc-text-muted border-pc-border hover:text-pc-text',
                  ].join(' ')}
                >
                  {recoveryLabel(s)} ({count})
                </button>
              );
            })}
          </div>

          <Card className="overflow-hidden p-0">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-pc-border text-left text-xs uppercase tracking-wide text-pc-text-muted">
                  <th className="px-4 py-2 font-medium">Task</th>
                  <th className="px-4 py-2 font-medium">Agent</th>
                  <th className="px-4 py-2 font-medium">Status</th>
                  <th className="px-4 py-2 font-medium">Error</th>
                  <th className="px-4 py-2 font-medium">Updated</th>
                  <th className="px-4 py-2 font-medium">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-pc-border">
                {filtered.map((task) => {
                  const busy = actionBusy === task.task_id;
                  return (
                    <tr key={task.task_id} className="hover:bg-pc-elevated/50">
                      <td className="px-4 py-2">
                        <Link to="/tasks" className="text-xs font-mono text-pc-accent hover:underline" title={task.task_id}>
                          {task.task_id.substring(0, 12)}…
                        </Link>
                      </td>
                      <td className="px-4 py-2">
                        <Link
                          to={`/agent/${encodeURIComponent(task.owner_agent)}`}
                          className="text-xs text-pc-accent hover:underline"
                        >
                          {task.owner_agent}
                        </Link>
                      </td>
                      <td className="px-4 py-2">
                        <Badge tone={recoveryTone(task.status)}>{recoveryLabel(task.status)}</Badge>
                      </td>
                      <td className="px-4 py-2 max-w-[200px]">
                        {task.error ? (
                          <span className="text-xs text-pc-text-muted truncate block" title={task.error}>
                            {task.error.substring(0, 80)}{task.error.length > 80 ? '…' : ''}
                          </span>
                        ) : (
                          <span className="text-xs text-pc-text-muted">—</span>
                        )}
                      </td>
                      <td className="px-4 py-2 text-xs text-pc-text-muted whitespace-nowrap">
                        {formatRelative(task.updated_at)}
                      </td>
                      <td className="px-4 py-2">
                        <div className="flex items-center gap-1">
                          {canRetry(task.status) && (
                            <Button size="sm" variant="ghost" disabled={busy} onClick={() => runAction(task.task_id, reopenTask(task.task_id), 'Retry')}>
                              {task.status === 'failed' ? 'Retry' : 'Review'}
                            </Button>
                          )}
                          {canAcknowledge(task.status) && (
                            <Button size="sm" variant="ghost" disabled={busy} onClick={() => runAction(task.task_id, acknowledgeTask(task.task_id), 'Acknowledge')}>
                              Acknowledge
                            </Button>
                          )}
                          <Button size="sm" variant="ghost" disabled={busy} onClick={() => setCancelTarget(task.task_id)}>
                            Cancel
                          </Button>
                        </div>
                      </td>
                    </tr>
                  );
                })}
                {filtered.length === 0 && (
                  <tr>
                    <td colSpan={6} className="px-4 py-8 text-center text-xs text-pc-text-muted italic">
                      No tasks match the selected filter.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </Card>
        </>
      )}

      <ConfirmDialog
        open={cancelTarget !== null}
        title="Cancel task"
        message="Cancel this task? This will request cancellation and cascade to subtasks."
        confirmLabel="Cancel task"
        danger
        onConfirm={handleCancel}
        onClose={() => setCancelTarget(null)}
      />
    </div>
  );
}
