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

export interface PickedFile {
  name: string;
  path: string;
  size_bytes: number;
  extension: string;
}

export interface DocInfo {
  id: number;
  page_count: number;
  first_page: [number, number];
  path: string;
  name: string;
}

export interface LayoutPage {
  index: number;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface LayoutJson {
  rev: number;
  mode: string;
  rotation: number;
  doc_w: number;
  doc_h: number;
  pages: LayoutPage[];
}

export interface PollResult {
  scroll: [number, number];
  zoom: number;
  zoom_step: number;
  active_page: number;
  layout: LayoutJson | null;
  ready_tiles: string[];
  thumbs_ready: [number, number, number][];
  cache_tiles: number;
  cache_bytes: number;
  rendered_total: number;
  stale_dropped: number;
}

export interface OutlineNode {
  title: string;
  page: number | null;
  children: OutlineNode[];
}

export function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("vdf_app_info");
}

export function pickPdfDialog(): Promise<PickedFile | null> {
  return invoke<PickedFile | null>("vdf_open_file");
}

export function openDocument(path: string): Promise<DocInfo> {
  return invoke<DocInfo>("vdf_open_document", { path });
}

export function closeDocument(id: number): Promise<void> {
  return invoke<void>("vdf_close_document", { id });
}

export function sendViewport(args: {
  id: number;
  viewportW: number;
  viewportH: number;
  dpr: number;
  scrollX: number;
  scrollY: number;
  velocityY: number;
  interacting: boolean;
}): Promise<void> {
  return invoke<void>("vdf_viewport", {
    id: args.id,
    viewport_w: args.viewportW,
    viewport_h: args.viewportH,
    dpr: args.dpr,
    scroll_x: args.scrollX,
    scroll_y: args.scrollY,
    velocity_y: args.velocityY,
    interacting: args.interacting,
  });
}

export function poll(id: number, layoutRevSeen: number): Promise<PollResult> {
  return invoke<PollResult>("vdf_poll", { id, layoutRevSeen });
}

export function pan(id: number, dx: number, dy: number): Promise<void> {
  return invoke<void>("vdf_pan", { id, dx, dy });
}

export function zoom(
  id: number,
  factor: number,
  focalX: number,
  focalY: number,
  absolute?: number,
): Promise<void> {
  return invoke<void>("vdf_zoom", {
    id,
    factor,
    focal_x: focalX,
    focal_y: focalY,
    absolute: absolute ?? null,
  });
}

export function fitWidth(id: number): Promise<number> {
  return invoke<number>("vdf_fit_width", { id });
}

export function setPageMode(id: number, mode: string): Promise<void> {
  return invoke<void>("vdf_set_page_mode", { id, mode });
}

export function rotate(id: number, quarterTurns: number): Promise<void> {
  return invoke<void>("vdf_rotate", { id, quarterTurns });
}

export function getOutline(id: number): Promise<OutlineNode[]> {
  return invoke<OutlineNode[]>("vdf_outline", { id });
}

export function gotoPage(id: number, page: number): Promise<[number, number]> {
  return invoke<[number, number]>("vdf_goto_page", { id, page });
}

export function requestThumbnails(id: number, pages: number[], width: number): Promise<void> {
  return invoke<void>("vdf_request_thumbnails", { id, pages, width });
}

/**
 * URL for a tile's raw bytes on the custom URI scheme. Windows uses the
 * http:// scheme host form; Linux/macOS the scheme:// form.
 */
export function tileUrl(docId: number, key: string): string {
  const host = navigator.platform.toLowerCase().includes("win")
    ? `http://vdf-tile.${import.meta.env.VITE_TAURI_LOCALHOST ?? "localhost"}/d${docId}/${key}`
    : `vdf-tile://localhost/d${docId}/${key}`;
  return host;
}

export function thumbUrl(docId: number, page: number, width: number): string {
  const host = navigator.platform.toLowerCase().includes("win")
    ? `http://vdf-thumb.${import.meta.env.VITE_TAURI_LOCALHOST ?? "localhost"}/${docId}/${page}/${width}`
    : `vdf-thumb://localhost/${docId}/${page}/${width}`;
  return host;
}
