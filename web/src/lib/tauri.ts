// Tauri detection utilities for ClawCrew Desktop.

declare global {
  interface Window {
    __TAURI__?: unknown;
    __CLAWCREW_GATEWAY__?: string;
  }
}

/** Returns true when running inside a Tauri WebView. */
export const isTauri = (): boolean => '__TAURI__' in window;

/** Gateway base URL when running inside Tauri (defaults to localhost). */
export const tauriGatewayUrl = (): string =>
  window.__CLAWCREW_GATEWAY__ ?? 'http://127.0.0.1:42617';

type TauriBridge = {
  core?: {
    invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
  };
};

export function invokeDesktop<T>(command: string): Promise<T> {
  const bridge = window.__TAURI__ as TauriBridge | undefined;
  if (!bridge?.core?.invoke) {
    return Promise.reject(new Error('Desktop IPC is unavailable'));
  }
  return bridge.core.invoke<T>(command);
}
