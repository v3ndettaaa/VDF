/**
 * Typed bridge to the Rust shell (vdf-app).
 *
 * In the packaged/dev Tauri webview, `__TAURI_INTERNALS__.invoke` is the
 * IPC entry. Everywhere else (plain browser, vitest) it is absent and every
 * call fails loudly — that honesty matters: silent stubs are how fake
 * functionality sneaks in.
 */

interface TauriInternals {
  invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
}

declare global {
  interface Window {
    __TAURI_INTERNALS__?: TauriInternals;
  }
}

export class IpcUnavailableError extends Error {
  constructor() {
    super("Tauri bridge unavailable: VDF commands only work inside the VDF shell");
    this.name = "IpcUnavailableError";
  }
}

export function hasTauriBridge(): boolean {
  return typeof window !== "undefined" && !!window.__TAURI_INTERNALS__;
}

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const internals = typeof window !== "undefined" ? window.__TAURI_INTERNALS__ : undefined;
  if (!internals) throw new IpcUnavailableError();
  return internals.invoke(cmd, args) as Promise<T>;
}

export interface AppInfo {
  name: string;
  version: string;
  platform: string;
  arch: string;
  schema_version: number;
  metrics_registered: number;
}

export interface OpenedFileInfo {
  name: string;
  size_bytes: number;
  extension: string;
}

export function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("vdf_app_info");
}

export function openNativePdf(): Promise<OpenedFileInfo | null> {
  return invoke<OpenedFileInfo | null>("vdf_open_file");
}
