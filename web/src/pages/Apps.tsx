import { useCallback, useState } from 'react';
import { Loader2, RefreshCw, Trash2 } from 'lucide-react';
import {
  getApps,
  enableApp,
  disableApp,
  updateApp,
  rollbackApp,
  removeApp,
  type AppManifest,
} from '@/lib/api';
import { usePolling } from '@/hooks/usePolling';
import { formatRelative } from '@/lib/format';
import { appStateTone, canRemoveApp, verdictTone } from './apps.logic';
import { Badge, Button, Card, ConfirmDialog, PageHeader, StatCard } from '@/components/ui';

export default function Apps() {
  const [apps, setApps] = useState<AppManifest[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionBusy, setActionBusy] = useState<string | null>(null);
  const [removeTarget, setRemoveTarget] = useState<AppManifest | null>(null);

  const fetchApps = useCallback(async (isStale: () => boolean) => {
    try {
      const result = await getApps();
      if (!isStale()) {
        setApps(result.apps);
        setError(null);
      }
    } catch (cause) {
      if (!isStale()) setError(cause instanceof Error ? cause.message : 'Unable to load apps');
    } finally {
      if (!isStale()) setLoading(false);
    }
  }, []);

  usePolling(fetchApps, 5_000);

  const runAction = useCallback(
    (appId: string, action: Promise<unknown>, label: string) => {
      setActionBusy(appId);
      action
        .then(() => void fetchApps(() => false))
        .catch((cause) =>
          setError(cause instanceof Error ? cause.message : `${label} failed`),
        )
        .finally(() => setActionBusy(null));
    },
    [fetchApps],
  );

  const handleRemove = useCallback(() => {
    if (!removeTarget) return;
    runAction(removeTarget.id, removeApp(removeTarget.id), 'Remove');
    setRemoveTarget(null);
  }, [removeTarget, runAction]);

  const enabled = apps.filter((a) => a.state === 'enabled');
  const disabled = apps.filter((a) => a.state === 'disabled');

  return (
    <div className="space-y-4">
      <PageHeader
        title="Apps"
        description="Install and manage ClawCrew Apps — MCP servers, tools, and extensions."
        actions={
          <Button variant="ghost" size="sm" onClick={() => void fetchApps(() => false)} disabled={loading}>
            {loading ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden /> : <RefreshCw className="h-4 w-4" aria-hidden />}
            Refresh
          </Button>
        }
      />

      {error ? (
        <Card className="border-status-error/30 text-sm text-status-error">
          <p>Apps data is unavailable: {error}</p>
          <p className="mt-1 text-xs text-pc-text-muted">
            The Apps registry endpoint must be wired before this view can display installed apps.
          </p>
        </Card>
      ) : loading && apps.length === 0 ? (
        <div className="flex items-center justify-center py-16 text-pc-text-muted">
          <Loader2 className="mr-2 h-5 w-5 animate-spin" aria-hidden />
          Loading apps…
        </div>
      ) : apps.length === 0 ? (
        <Card className="p-6 text-center text-sm" style={{ color: 'var(--pc-text-faint)' }}>
          <p>No apps installed. Apps can be installed via the CLI or configuration.</p>
        </Card>
      ) : (
        <>
          <div className="grid gap-4 md:grid-cols-3">
            <StatCard label="Total apps" value={apps.length} />
            <StatCard label="Enabled" value={enabled.length} tone="ok" />
            <StatCard label="Disabled" value={disabled.length} tone="warn" />
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {apps.map((app) => {
              const busy = actionBusy === app.id;
              const isEnabled = app.state === 'enabled';
              return (
                <Card key={app.id}>
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0">
                      <h3 className="text-sm font-semibold text-pc-text truncate">{app.name}</h3>
                      <p className="text-xs text-pc-text-muted mt-0.5">v{app.version}</p>
                    </div>
                    <Badge tone={appStateTone(app.state)}>{app.state}</Badge>
                  </div>

                  {app.min_runtime_version && (
                    <p className="mt-2 text-[11px] text-pc-text-muted">
                      Min runtime: <span className="font-mono">{app.min_runtime_version}</span>
                    </p>
                  )}

                  {app.dependencies && app.dependencies.length > 0 && (
                    <div className="mt-2 flex flex-wrap gap-1">
                      {app.dependencies.map((dep) => (
                        <Badge key={dep}>{dep}</Badge>
                      ))}
                    </div>
                  )}

                  {app.mcp_server && (
                    <div className="mt-3 rounded border border-pc-border bg-pc-elevated p-2 text-xs">
                      <span className="font-semibold text-pc-text-secondary">MCP Server</span>
                      <div className="mt-1 font-mono text-pc-text-muted truncate" title={`${app.mcp_server.command} ${app.mcp_server.args.join(' ')}`}>
                        {app.mcp_server.command} {app.mcp_server.args.join(' ')}
                      </div>
                    </div>
                  )}

                  {app.permissions && app.permissions.length > 0 && (
                    <div className="mt-3 border-t border-pc-border pt-2">
                      <h4 className="text-[11px] font-medium uppercase tracking-wide text-pc-text-faint">Tool permissions</h4>
                      <div className="mt-1 space-y-0.5">
                        {app.permissions.map((perm) => (
                          <div key={perm.tool_name} className="flex items-center justify-between gap-2 text-xs">
                            <code className="text-pc-text-muted truncate">{perm.tool_name}</code>
                            <Badge tone={verdictTone(perm.verdict)}>{perm.verdict}</Badge>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {app.tools && app.tools.length > 0 && !app.permissions?.length && (
                    <div className="mt-3 border-t border-pc-border pt-2">
                      <h4 className="text-[11px] font-medium uppercase tracking-wide text-pc-text-faint">Tools</h4>
                      <div className="mt-1 flex flex-wrap gap-1">
                        {app.tools.map((tool) => (
                          <Badge key={tool}>{tool}</Badge>
                        ))}
                      </div>
                    </div>
                  )}

                  <div className="mt-4 flex flex-wrap items-center gap-2 border-t border-pc-border pt-3">
                    {isEnabled ? (
                      <Button size="sm" variant="ghost" disabled={busy} onClick={() => runAction(app.id, disableApp(app.id), 'Disable')}>
                        Disable
                      </Button>
                    ) : (
                      <Button size="sm" variant="primary" disabled={busy} onClick={() => runAction(app.id, enableApp(app.id), 'Enable')}>
                        Enable
                      </Button>
                    )}
                    <Button size="sm" variant="ghost" disabled={busy} onClick={() => runAction(app.id, updateApp(app.id), 'Update')}>
                      Update
                    </Button>
                    <Button size="sm" variant="ghost" disabled={busy} onClick={() => runAction(app.id, rollbackApp(app.id), 'Rollback')}>
                      Rollback
                    </Button>
                    {canRemoveApp(app.state) && (
                      <Button size="sm" variant="ghost" disabled={busy} onClick={() => setRemoveTarget(app)}>
                        <Trash2 className="h-3.5 w-3.5" aria-hidden />
                        Remove
                      </Button>
                    )}
                  </div>

                  {app.updated_at && (
                    <p className="mt-2 text-[11px] text-pc-text-muted">
                      Updated {formatRelative(app.updated_at)}
                    </p>
                  )}
                </Card>
              );
            })}
          </div>
        </>
      )}

      <ConfirmDialog
        open={removeTarget !== null}
        title="Remove app"
        message={removeTarget ? `Remove "${removeTarget.name}" (v${removeTarget.version})? This cannot be undone.` : undefined}
        confirmLabel="Remove"
        danger
        onConfirm={handleRemove}
        onClose={() => setRemoveTarget(null)}
      />
    </div>
  );
}
