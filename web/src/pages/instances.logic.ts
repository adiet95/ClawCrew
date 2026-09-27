export function healthTone(health: string): 'ok' | 'warn' | 'error' | 'neutral' {
  switch (health) {
    case 'healthy': return 'ok';
    case 'degraded': return 'warn';
    case 'offline': return 'error';
    default: return 'neutral';
  }
}

export function healthLabel(health: string): string {
  return health.charAt(0).toUpperCase() + health.slice(1);
}

export function isOnline(health: string): boolean {
  return health === 'healthy' || health === 'degraded';
}
