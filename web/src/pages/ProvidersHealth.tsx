import { useCallback, useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { Loader2, RefreshCw } from 'lucide-react';
import { getProvidersHealth, type ProviderHealthRow } from '@/lib/api';
import { formatRelative } from '@/lib/format';
import { Badge, Button, Card, PageHeader } from '@/components/ui';

function statusTone(status: string): 'neutral' | 'ok' | 'warn' | 'error' {
  switch (status.toLowerCase()) {
    case 'ok':
      return 'ok';
    case 'starting':
      return 'warn';
    case 'error':
      return 'error';
    default:
      return 'neutral';
  }
}

export default function ProvidersHealth() {
  const [providers, setProviders] = useState<ProviderHealthRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const result = await getProvidersHealth();
      setProviders(result.providers);
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Unable to load provider health');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
    const interval = window.setInterval(() => void load(), 10_000);
    return () => window.clearInterval(interval);
  }, [load]);

  return (
    <div className="space-y-4">
      <PageHeader
        title="Provider Health"
        description="Per-agent resolved provider and live health-registry status."
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
      ) : providers.length === 0 ? (
        <Card className="p-8 text-center text-sm text-pc-text-muted">
          No agents configured.
        </Card>
      ) : (
        <Card className="overflow-hidden p-0">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-pc-border text-left text-xs uppercase tracking-wide text-pc-text-muted">
                <th className="px-4 py-2.5 font-medium">Agent</th>
                <th className="px-4 py-2.5 font-medium">Provider</th>
                <th className="px-4 py-2.5 font-medium">Model</th>
                <th className="px-4 py-2.5 font-medium">Status</th>
                <th className="px-4 py-2.5 font-medium">Avg latency</th>
                <th className="px-4 py-2.5 font-medium">Last OK</th>
                <th className="px-4 py-2.5 font-medium">Last Error</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-pc-border">
              {providers.map((row) => (
                <tr key={row.agent} className="hover:bg-pc-elevated/50">
                  <td className="px-4 py-2.5 font-medium">
                    <Link to={`/agent/${encodeURIComponent(row.agent)}`} className="text-pc-accent hover:underline">
                      {row.agent}
                    </Link>
                  </td>
                  <td className="px-4 py-2.5 text-pc-text-secondary">{row.provider ?? '—'}</td>
                  <td className="px-4 py-2.5 font-mono text-xs text-pc-text-secondary">{row.model || '—'}</td>
                  <td className="px-4 py-2.5">
                    <Badge tone={statusTone(row.status)}>{row.status}</Badge>
                  </td>
                  <td className="px-4 py-2.5 tabular-nums text-pc-text-muted">
                    {row.avg_latency_ms != null ? `${Math.round(row.avg_latency_ms)}ms` : '—'}
                  </td>
                  <td className="px-4 py-2.5 text-pc-text-muted">
                    {row.last_ok ? formatRelative(row.last_ok) : '—'}
                  </td>
                  <td className="max-w-[18rem] truncate px-4 py-2.5 text-status-error" title={row.last_error ?? ''}>
                    {row.last_error ?? ''}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="border-t border-pc-border px-4 py-2 text-[11px] text-pc-text-muted">
            Status <code>unknown</code> means no health probe has been recorded for the provider.
          </p>
        </Card>
      )}
    </div>
  );
}
