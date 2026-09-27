import { useCallback, useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { Loader2, RefreshCw } from 'lucide-react';
import {
  getProvidersHealth,
  getRunningSessions,
  getSessions,
  type ProviderHealthRow,
} from '@/lib/api';
import { apiOrigin, basePath } from '@/lib/basePath';
import { SSEClient } from '@/lib/sse';
import type { Session } from '@/types/api';
import { formatRelative } from '@/lib/format';
import { Badge, Button, Card, PageHeader } from '@/components/ui';

function providerTone(status: string): 'neutral' | 'ok' | 'warn' | 'error' {
  switch (status) {
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

export default function SessionsHealth() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [running, setRunning] = useState<Set<string>>(new Set());
  const [fallbacks, setFallbacks] = useState<Map<string, number>>(new Map());
  const [compactions, setCompactions] = useState<Map<string, number>>(new Map());
  const [providers, setProviders] = useState<ProviderHealthRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [all, active, providerHealth] = await Promise.all([
        getSessions(),
        getRunningSessions(),
        getProvidersHealth(),
      ]);
      setSessions(all);
      setRunning(new Set(active.sessions.map((s) => s.session_id)));
      setFallbacks(new Map(active.sessions.map((s) => [s.session_id, s.provider_fallbacks ?? 0])));
      setCompactions(new Map(active.sessions.map((s) => [s.session_id, s.compactions ?? 0])));
      setProviders(providerHealth.providers);
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Unable to load session health');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
    const interval = window.setInterval(() => void load(), 5_000);
    return () => window.clearInterval(interval);
  }, [load]);

  // Live session activity over SSE: the server emits a `sessions` frame only on
  // change, so the active/idle and fallback columns stay current between polls.
  useEffect(() => {
    const client = new SSEClient({ path: `${apiOrigin}${basePath}/api/sessions/stream` });
    client.onEvent = (frame) => {
      if (frame.type !== 'sessions' || !Array.isArray(frame.sessions)) return;
      const list = frame.sessions as Array<{ session_id: string; provider_fallbacks?: number; compactions?: number }>;
      setRunning(new Set(list.map((s) => s.session_id)));
      setFallbacks(new Map(list.map((s) => [s.session_id, s.provider_fallbacks ?? 0])));
      setCompactions(new Map(list.map((s) => [s.session_id, s.compactions ?? 0])));
    };
    client.connect();
    return () => client.disconnect();
  }, []);

  const providerByAgent = new Map(providers.map((p) => [p.agent, p.status]));

  return (
    <div className="space-y-4">
      <PageHeader
        title="Session Health"
        description="Active/idle sessions and the health of their owning agent's provider."
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
      ) : loading && sessions.length === 0 ? (
        <div className="flex items-center justify-center py-16 text-pc-text-muted">
          <Loader2 className="mr-2 h-5 w-5 animate-spin" aria-hidden />
          Loading sessions...
        </div>
      ) : sessions.length === 0 ? (
        <Card className="p-8 text-center text-sm text-pc-text-muted">
          No sessions recorded.
        </Card>
      ) : (
        <Card className="overflow-hidden p-0">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-pc-border text-left text-xs uppercase tracking-wide text-pc-text-muted">
                <th className="px-4 py-2.5 font-medium">Session</th>
                <th className="px-4 py-2.5 font-medium">Agent</th>
                <th className="px-4 py-2.5 font-medium">Channel</th>
                <th className="px-4 py-2.5 font-medium">Activity</th>
                <th className="px-4 py-2.5 font-medium">Provider</th>
                <th className="px-4 py-2.5 font-medium">Fallbacks</th>
                <th className="px-4 py-2.5 font-medium">Compactions</th>
                <th className="px-4 py-2.5 font-medium">Messages</th>
                <th className="px-4 py-2.5 font-medium">Last Activity</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-pc-border">
              {sessions.map((session) => {
                const active = running.has(session.session_id);
                const providerStatus = session.agent_alias
                  ? providerByAgent.get(session.agent_alias) ?? 'unknown'
                  : 'n/a';
                return (
                  <tr key={session.session_key} className="hover:bg-pc-elevated/50">
                    <td className="max-w-[16rem] truncate px-4 py-2.5 font-medium text-pc-text" title={session.session_id}>
                      {session.name ?? session.session_id}
                    </td>
                    <td className="px-4 py-2.5 text-pc-text-secondary">
                      {session.agent_alias ? (
                        <Link
                          to={`/agent/${encodeURIComponent(session.agent_alias)}`}
                          className="text-pc-accent hover:underline"
                        >
                          {session.agent_alias}
                        </Link>
                      ) : (
                        '—'
                      )}
                    </td>
                    <td className="px-4 py-2.5 text-pc-text-secondary">
                      {session.channel_id ? (
                        <Link to="/integrations" className="text-pc-accent hover:underline">
                          {session.channel_id}
                        </Link>
                      ) : (
                        '—'
                      )}
                    </td>
                    <td className="px-4 py-2.5">
                      <Badge tone={active ? 'ok' : 'neutral'}>{active ? 'active' : 'idle'}</Badge>
                    </td>
                    <td className="px-4 py-2.5">
                      <Badge tone={providerTone(providerStatus)}>{providerStatus}</Badge>
                    </td>
                    <td className="px-4 py-2.5 tabular-nums text-pc-text-secondary">
                      {fallbacks.get(session.session_id) ?? 0}
                    </td>
                    <td className="px-4 py-2.5 tabular-nums text-pc-text-secondary">
                      {compactions.get(session.session_id) ?? 0}
                    </td>
                    <td className="px-4 py-2.5 tabular-nums text-pc-text-secondary">{session.message_count}</td>
                    <td className="px-4 py-2.5 text-pc-text-muted">{formatRelative(session.last_activity)}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </Card>
      )}
    </div>
  );
}
