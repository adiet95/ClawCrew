import { useCallback, useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { Check, Loader2, RefreshCw, X } from 'lucide-react';
import {
  approveSopRun,
  denySopRun,
  getPendingApprovals,
  type PendingApproval,
} from '@/lib/api';
import { formatRelative } from '@/lib/format';
import { Badge, Button, Card, PageHeader } from '@/components/ui';

export default function Approvals() {
  const [pending, setPending] = useState<PendingApproval[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const result = await getPendingApprovals();
      setPending(result.pending);
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Unable to load pending approvals');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
    const interval = window.setInterval(() => void load(), 5_000);
    return () => window.clearInterval(interval);
  }, [load]);

  const act = useCallback(
    (runId: string, action: Promise<unknown>) => {
      setBusy(runId);
      action
        .then(() => load())
        .catch((cause) => setError(cause instanceof Error ? cause.message : 'Action failed'))
        .finally(() => setBusy(null));
    },
    [load],
  );

  return (
    <div className="space-y-4">
      <PageHeader
        title="Approvals"
        description="Background runs parked on a human decision."
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
      ) : loading && pending.length === 0 ? (
        <div className="flex items-center justify-center py-16 text-pc-text-muted">
          <Loader2 className="mr-2 h-5 w-5 animate-spin" aria-hidden />
          Loading approvals...
        </div>
      ) : pending.length === 0 ? (
        <Card className="p-8 text-center text-sm text-pc-text-muted">
          No runs are waiting for approval.
        </Card>
      ) : (
        <Card className="overflow-hidden p-0">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-pc-border text-left text-xs uppercase tracking-wide text-pc-text-muted">
                <th className="px-4 py-2.5 font-medium">SOP</th>
                <th className="px-4 py-2.5 font-medium">Kind</th>
                <th className="px-4 py-2.5 font-medium">Step</th>
                <th className="px-4 py-2.5 font-medium">Waiting</th>
                <th className="px-4 py-2.5" />
              </tr>
            </thead>
            <tbody className="divide-y divide-pc-border">
              {pending.map((row) => (
                <tr key={row.run_id} className="hover:bg-pc-elevated/50">
                  <td className="px-4 py-2.5 font-medium text-pc-text">
                    <Link
                      to={`/runs/${encodeURIComponent(row.sop_name)}/${encodeURIComponent(row.run_id)}`}
                      className="text-pc-accent hover:underline"
                    >
                      {row.sop_name}
                    </Link>
                  </td>
                  <td className="px-4 py-2.5">
                    <Badge>{row.kind}</Badge>
                  </td>
                  <td className="px-4 py-2.5 tabular-nums text-pc-text-secondary">
                    {row.step}/{row.total_steps}
                  </td>
                  <td className="px-4 py-2.5 text-pc-text-muted">
                    {row.waiting_since ? formatRelative(row.waiting_since) : '—'}
                  </td>
                  <td className="px-4 py-2.5 text-right">
                    <div className="flex items-center justify-end gap-2">
                      <Button
                        size="sm"
                        variant="ghost"
                        disabled={busy === row.run_id}
                        onClick={() => act(row.run_id, approveSopRun(row.run_id))}
                      >
                        {busy === row.run_id ? (
                          <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />
                        ) : (
                          <Check className="h-3.5 w-3.5" aria-hidden />
                        )}
                        Approve
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        disabled={busy === row.run_id}
                        onClick={() => act(row.run_id, denySopRun(row.run_id))}
                      >
                        <X className="h-3.5 w-3.5" aria-hidden />
                        Deny
                      </Button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}
    </div>
  );
}
