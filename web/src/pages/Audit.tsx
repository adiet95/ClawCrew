import { useCallback, useEffect, useState } from 'react';
import { Loader2, RefreshCw, ShieldCheck, ShieldAlert } from 'lucide-react';
import { getAuditFeed, type AuditFeed } from '@/lib/api';
import { formatRelative } from '@/lib/format';
import { Badge, Button, Card, PageHeader } from '@/components/ui';

function field(entry: Record<string, unknown>, key: string): string | null {
  const value = entry[key];
  return typeof value === 'string' ? value : null;
}

export default function Audit() {
  const [feed, setFeed] = useState<AuditFeed | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setFeed(await getAuditFeed(100));
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Unable to load the audit feed');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="space-y-4">
      <PageHeader
        title="Audit"
        description="Tamper-evident security audit feed (redacted)."
        actions={
          <Button variant="ghost" size="sm" onClick={() => void load()} disabled={loading}>
            {loading ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden /> : <RefreshCw className="h-4 w-4" aria-hidden />}
            Refresh
          </Button>
        }
      />

      {error ? (
        <Card className="border-status-error/30 text-sm text-status-error">
          <p>{error}</p>
        </Card>
      ) : !feed ? (
        <div className="flex items-center justify-center py-16 text-pc-text-muted">
          <Loader2 className="mr-2 h-5 w-5 animate-spin" aria-hidden />
          Loading audit feed...
        </div>
      ) : (
        <>
          <div className="flex items-center gap-2 text-sm">
            {feed.verified ? (
              <span className="inline-flex items-center gap-1 text-status-success">
                <ShieldCheck className="h-4 w-4" aria-hidden /> Chain verified
              </span>
            ) : (
              <span className="inline-flex items-center gap-1 text-status-error">
                <ShieldAlert className="h-4 w-4" aria-hidden /> Chain verification FAILED
              </span>
            )}
            {!feed.enabled ? <Badge tone="warn">audit disabled</Badge> : null}
          </div>

          {feed.entries.length === 0 ? (
            <Card className="p-8 text-center text-sm text-pc-text-muted">
              No audit entries recorded.
            </Card>
          ) : (
            <Card className="space-y-2">
              {feed.entries.map((entry, index) => (
                <div
                  key={`${field(entry, 'event_id') ?? index}`}
                  className="rounded border border-pc-border bg-pc-elevated p-2 text-xs"
                >
                  <div className="flex flex-wrap items-center justify-between gap-2 text-pc-text-secondary">
                    <div className="flex items-center gap-2">
                      <Badge>{field(entry, 'event_type') ?? 'event'}</Badge>
                      {field(entry, 'agent_alias') ? (
                        <span className="text-pc-text-muted">{field(entry, 'agent_alias')}</span>
                      ) : null}
                    </div>
                    <span>{field(entry, 'timestamp') ? formatRelative(field(entry, 'timestamp')!) : ''}</span>
                  </div>
                  <pre className="mt-1 max-h-32 overflow-auto whitespace-pre-wrap text-[11px] text-pc-text-muted">
                    {JSON.stringify(entry)}
                  </pre>
                </div>
              ))}
            </Card>
          )}
        </>
      )}
    </div>
  );
}
