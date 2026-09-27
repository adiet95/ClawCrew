export function appStateTone(state: string): 'ok' | 'warn' | 'neutral' {
  switch (state) {
    case 'enabled': return 'ok';
    case 'disabled': return 'warn';
    default: return 'neutral';
  }
}

export function canRemoveApp(state: string): boolean {
  return state !== 'enabled';
}

export function verdictTone(verdict: string): 'ok' | 'error' | 'warn' {
  switch (verdict) {
    case 'allow': return 'ok';
    case 'deny': return 'error';
    default: return 'warn';
  }
}
