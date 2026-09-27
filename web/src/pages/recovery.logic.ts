export type RecoveryStatus = 'needs_review' | 'lost' | 'timed_out' | 'failed';

export const RECOVERY_STATUSES: readonly RecoveryStatus[] = ['needs_review', 'lost', 'timed_out', 'failed'];

export function recoveryTone(status: string): 'error' | 'warn' | 'neutral' {
  switch (status) {
    case 'lost':
    case 'failed':
      return 'error';
    case 'needs_review':
    case 'timed_out':
      return 'warn';
    default:
      return 'neutral';
  }
}

export function recoveryLabel(status: string): string {
  return status.replace(/_/g, ' ');
}

export function canRetry(status: string): boolean {
  return ['failed', 'needs_review'].includes(status);
}

export function canAcknowledge(status: string): boolean {
  return ['lost', 'timed_out'].includes(status);
}

export function filterByStatus<T extends { status: string }>(tasks: T[], filter: string | null): T[] {
  if (!filter) return tasks;
  return tasks.filter((t) => t.status === filter);
}
