import { useCallback, useState } from 'react';
import { Link } from 'react-router-dom';
import { Loader2, RefreshCw } from 'lucide-react';
import {
  getInstances,
  reconnectInstance,
  markInstanceOffline,
  type InstanceRecord,
} from '@/lib/api';
import { usePolling } from '@/hooks/usePolling';
import { formatRelative } from '@/lib/format';
import { healthTone, healthLabel, isOnline } from './instances.logic';
import { Badge, Button, Card, PageHeader, StatCard } from '@/components/ui';

export default function Instances() {
  const [instances, setInstances] = useState<InstanceRecord[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionBusy, setActionBusy] = useState<string | null>(null);

  const fetchInstances = useCallback(async (isStale: () => boolean) => {
    try {
      const result = await getInstances();
      if (!isStale()) {
        setInstances(result.instances);
        setError(null);
      }
    } catch (cause) {
      if (!isStale()) setError(cause instanceof Error ? cause.message : 'Unable to load instances');
    } finally {
      if (!isStale()) setLoading(false);
    }
  }, []);

  usePolling(fetchInstances, 5_000);

  const runAction = useCallback(
    (id: string, action: Promise<unknown>, label: string) => {
      setActionBusy(id);
      action
        .then(() => void fetchInstances(() => false))
        .catch((cause) =>
          setError(cause instanceof Error ? cause.message : `${label} failed`),
        )
        .finally(() => setActionBusy(null));
    },
    [fetchInstances],
  );

  const healthy = instances.filter((i) => i.health === 'healthy').length;
  const degraded = instances.filter((i) => i.health === 'degraded').length;
  const offline = instances.filter((i) => i.health === 'offline').length;

  return (
    <div className="space-y-4">
      <PageHeader
        title="Instances / Remote Crews"
        description="Manage remote crews and local instance routing."
        actions={
          <Button variant="ghost" size="sm" onClick={() => void fetchInstances(() => false)} disabled={loading}>
            {loading ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden /> : <RefreshCw className="h-4 w-4" aria-hidden />}
            Refresh
          </Button>
        }
      />

      {error ? (
        <Card className="border-status-error/30 text-sm text-status-error">
          <p>Instance data is unavailable: {error}</p>
          <p className="mt-1 text-xs text-pc-text-muted">
            The instance registry endpoint must be wired before this view can display remote crews.
          </p>
        </Card>
      ) : loading && instances.length === 0 ? (
        <div className="flex items-center justify-center py-16 text-pc-text-muted">
          <Loader2 className="mr-2 h-5 w-5 animate-spin" aria-hidden />
          Loading instances…
        </div>
      ) : instances.length === 0 ? (
        <Card className="p-6 text-center text-sm" style={{ color: 'var(--pc-text-faint)' }}>
          <p>No remote instances registered. Use the CLI to connect to remote crews.</p>
        </Card>
      ) : (
        <>
          <div className="grid gap-4 md:grid-cols-4">
            <StatCard label="Total" value={instances.length} />
            <StatCard label="Healthy" value={healthy} tone="ok" />
            <StatCard label="Degraded" value={degraded} tone="warn" />
            <StatCard label="Offline" value={offline} tone="error" />
          </div>

          <Card className="overflow-hidden p-0">
            <div className="border-b border-pc-border px-4 py-2.5">
              <h2 className="text-sm font-semibold text-pc-text">Instance registry</h2>
            </div>
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-pc-border text-left text-xs uppercase tracking-wide text-pc-text-muted">
                  <th className="px-4 py-2 font-medium">Instance</th>
                  <th className="px-4 py-2 font-medium">Health</th>
                  <th className="px-4 py-2 font-medium">Capabilities</th>
                  <th className="px-4 py-2 font-medium">Owner</th>
                  <th className="px-4 py-2 font-medium">Version</th>
                  <th className="px-4 py-2 font-medium">Last seen</th>
                  <th className="px-4 py-2 font-medium">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-pc-border">
                {instances.map((inst) => {
                  const busy = actionBusy === inst.id;
                  const online = isOnline(inst.health);
                  return (
                    <tr key={inst.id} className="hover:bg-pc-elevated/50">
                      <td className="px-4 py-2">
                        <div className="flex items-center gap-2">
                          <span
                            className={`h-2 w-2 shrink-0 rounded-full ${online ? 'bg-status-success' : 'bg-pc-text-muted'}`}
                            aria-hidden
                          />
                          <div className="min-w-0">
                            <code className="text-xs text-pc-text truncate block" title={inst.id}>
                              {inst.name ?? inst.id}
                            </code>
                            {inst.endpoint && (
                              <span className="text-[11px] text-pc-text-muted font-mono truncate block">{inst.endpoint}</span>
                            )}
                          </div>
                        </div>
                      </td>
                      <td className="px-4 py-2">
                        <Badge tone={healthTone(inst.health)}>{healthLabel(inst.health)}</Badge>
                      </td>
                      <td className="px-4 py-2">
                        <div className="flex flex-wrap gap-1">
                          {inst.capabilities.length > 0
                            ? inst.capabilities.map((cap) => <Badge key={cap}>{cap}</Badge>)
                            : <span className="text-xs text-pc-text-muted">—</span>}
                        </div>
                      </td>
                      <td className="px-4 py-2 text-xs text-pc-text-secondary">{inst.owner ?? '—'}</td>
                      <td className="px-4 py-2 text-xs text-pc-text-muted font-mono">{inst.version ?? '—'}</td>
                      <td className="px-4 py-2 text-xs text-pc-text-muted">{formatRelative(inst.last_seen ?? null)}</td>
                      <td className="px-4 py-2">
                        <div className="flex items-center gap-1">
                          {!online ? (
                            <Button size="sm" variant="ghost" disabled={busy} onClick={() => runAction(inst.id, reconnectInstance(inst.id), 'Reconnect')}>
                              Reconnect
                            </Button>
                          ) : (
                            <Button size="sm" variant="ghost" disabled={busy} onClick={() => runAction(inst.id, markInstanceOffline(inst.id), 'Offline')}>
                              Mark offline
                            </Button>
                          )}
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </Card>
        </>
      )}
    </div>
  );
}
