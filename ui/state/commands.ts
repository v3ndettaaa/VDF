/**
 * Built-in command list — the single registry behind the command palette,
 * keyboard shortcuts, and menus (M6). M0 registers the shell commands that
 * exist; document commands (undo/zoom-to-page/etc.) join in later milestones.
 */

import { app, type AppState } from "./app";
import { openNativePdf } from "../app/ipc";

export interface CommandDef {
  id: string;
  title: string;
  /** Shortcut spec: "Ctrl+K" (Ctrl = Ctrl or ⌘). Shown in palette, wired by shortcuts/. */
  shortcut?: string;
  /** Hide from palette/shortcuts unless the predicate holds. */
  when?: (state: AppState) => boolean;
  run: () => void | Promise<void>;
}

/** Opens a PDF via the native dialog and registers the tab (M0: name+size only). */
export async function openPdfFlow(): Promise<void> {
  try {
    const info = await openNativePdf();
    if (!info) {
      app.setStatus("Open cancelled");
      return;
    }
    app.set({ lastOpenedFile: info });
    app.openDocumentTab(info.name);
    app.setStatus(`Opened ${info.name} (${formatBytes(info.size_bytes)})`);
  } catch (err) {
    app.setStatus(`Open failed: ${String(err)}`);
  }
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MiB`;
}

export const COMMANDS: CommandDef[] = [
  { id: "file.open", title: "File: Open…", shortcut: "Ctrl+O", run: openPdfFlow },
  { id: "view.theme.dark", title: "Theme: Dark", run: () => app.setTheme("dark") },
  { id: "view.theme.light", title: "Theme: Light", run: () => app.setTheme("light") },
  { id: "view.theme.system", title: "Theme: System", run: () => app.setTheme("system") },
  { id: "view.theme.cycle", title: "Theme: Cycle Dark/Light/System", shortcut: "Ctrl+Shift+T", run: () => app.cycleTheme() },
  { id: "view.sidebar.toggle", title: "View: Toggle Sidebar", shortcut: "Ctrl+B", run: () => app.toggleSidebar() },
  { id: "view.commandPalette", title: "Command Palette…", shortcut: "Ctrl+K", run: () => void import("../command-palette").then((m) => m.openPalette()) },
  { id: "zoom.in", title: "Zoom: In", shortcut: "Ctrl+=", run: () => app.zoomBy(1.1) },
  { id: "zoom.out", title: "Zoom: Out", shortcut: "Ctrl+-", run: () => app.zoomBy(1 / 1.1) },
  { id: "zoom.reset", title: "Zoom: Reset to 100%", shortcut: "Ctrl+0", run: () => app.resetZoom() },
  ...["select", "pen", "eraser", "highlighter", "text", "shape", "image"].map((tool) => ({
    id: `tool.${tool}`,
    title: `Tool: ${tool[0]!.toUpperCase()}${tool.slice(1)}`,
    run: () => app.setTool(tool as AppState["activeTool"]),
  })),
];

/** Case-insensitive substring filter over title and id. Empty query → all. */
export function filterCommands(query: string): CommandDef[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...COMMANDS];
  return COMMANDS.filter(
    (c) => c.title.toLowerCase().includes(q) || c.id.toLowerCase().includes(q),
  );
}

/** Finds the first command bound to a shortcut spec. */
export function commandForShortcut(spec: string): CommandDef | undefined {
  return COMMANDS.find((c) => c.shortcut === spec);
}
