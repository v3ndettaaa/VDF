/**
 * Built-in command list — the single registry behind the command palette,
 * keyboard shortcuts, and menus (M6). M1 adds viewer commands on top of the
 * M0 shell commands.
 */

import { app } from "./app";
import { pickPdfDialog, openDocument, closeDocument, fitWidth, setPageMode } from "../app/ipc";
import { controller } from "../viewport";

export interface CommandDef {
  id: string;
  title: string;
  /** Shortcut spec: "Ctrl+K" (Ctrl = Ctrl or ⌘). Shown in palette, wired by shortcuts/. */
  shortcut?: string;
  /** Hidden from palette/shortcuts unless the predicate holds. */
  when?: () => boolean;
  run: () => void | Promise<void>;
}

/** Opens a PDF via the native dialog and loads it into the shell. */
export async function openPdfFlow(): Promise<void> {
  try {
    const picked = await pickPdfDialog();
    if (!picked) {
      app.setStatus("Open cancelled");
      return;
    }
    const info = await openDocument(picked.path);
    app.set({ lastOpenedFile: picked });
    app.openDocumentTab(info.name);
    app.setActiveDoc({ id: info.id, name: info.name, pageCount: info.page_count });
    app.setStatus(
      `Opened ${info.name} — ${info.page_count} pages (${formatBytes(picked.size_bytes)})`,
    );
  } catch (err) {
    const msg = String(err);
    if (msg.includes("encrypted")) {
      app.setStatus("This PDF is password protected — unlock support arrives in M5");
    } else if (msg.startsWith("malformed")) {
      app.setStatus(`Could not open: ${msg}`);
    } else {
      app.setStatus(`Open failed: ${msg}`);
    }
  }
}

export async function closeActiveDoc(): Promise<void> {
  const doc = app.get().activeDoc;
  if (!doc) return;
  try {
    await closeDocument(doc.id);
  } finally {
    app.setActiveDoc(null);
    app.setStatus("Document closed");
  }
}

export function commandContext() {
  return {
    get docOpen(): boolean {
      return app.get().activeDoc !== null;
    },
    get controller() {
      return controller;
    },
  };
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MiB`;
}

export const COMMANDS: CommandDef[] = [
  { id: "file.open", title: "File: Open…", shortcut: "Ctrl+O", run: openPdfFlow },
  {
    id: "file.close",
    title: "File: Close Document",
    shortcut: "Ctrl+W",
    when: () => app.get().activeDoc !== null,
    run: closeActiveDoc,
  },
  { id: "view.theme.dark", title: "Theme: Dark", run: () => app.setTheme("dark") },
  { id: "view.theme.light", title: "Theme: Light", run: () => app.setTheme("light") },
  { id: "view.theme.system", title: "Theme: System", run: () => app.setTheme("system") },
  {
    id: "view.theme.cycle",
    title: "Theme: Cycle Dark/Light/System",
    shortcut: "Ctrl+Shift+T",
    run: () => app.cycleTheme(),
  },
  { id: "view.sidebar.toggle", title: "View: Toggle Sidebar", shortcut: "Ctrl+B", run: () => app.toggleSidebar() },
  {
    id: "view.commandPalette",
    title: "Command Palette…",
    shortcut: "Ctrl+K",
    run: () => void import("../command-palette").then((m) => m.openPalette()),
  },
  {
    id: "zoom.in",
    title: "Zoom: In",
    shortcut: "Ctrl+=",
    when: () => app.get().activeDoc !== null,
    run: () => controller?.zoomBy(1.2),
  },
  {
    id: "zoom.out",
    title: "Zoom: Out",
    shortcut: "Ctrl+-",
    when: () => app.get().activeDoc !== null,
    run: () => controller?.zoomBy(1 / 1.2),
  },
  {
    id: "zoom.reset",
    title: "Zoom: Reset to 100%",
    shortcut: "Ctrl+0",
    when: () => app.get().activeDoc !== null,
    run: () => controller?.zoomAbsolute(1),
  },
  {
    id: "zoom.fitWidth",
    title: "Zoom: Fit Width",
    shortcut: "Ctrl+1",
    when: () => app.get().activeDoc !== null,
    run: async () => {
      const doc = app.get().activeDoc;
      if (doc) controller?.zoomAbsolute(await fitWidth(doc.id));
    },
  },
  {
    id: "view.rotateCw",
    title: "View: Rotate Clockwise",
    shortcut: "Ctrl+R",
    when: () => app.get().activeDoc !== null,
    run: () => controller?.rotate(1),
  },
  {
    id: "mode.continuous",
    title: "Page Mode: Continuous",
    when: () => app.get().activeDoc !== null,
    run: () => {
      const doc = app.get().activeDoc;
      if (doc) void setPageMode(doc.id, "continuous");
    },
  },
  {
    id: "mode.single",
    title: "Page Mode: Single Page",
    when: () => app.get().activeDoc !== null,
    run: () => {
      const doc = app.get().activeDoc;
      if (doc) void setPageMode(doc.id, "single");
    },
  },
  {
    id: "mode.twoPage",
    title: "Page Mode: Two Pages",
    when: () => app.get().activeDoc !== null,
    run: () => {
      const doc = app.get().activeDoc;
      if (doc) void setPageMode(doc.id, "two-page");
    },
  },
  ...["select", "pen", "eraser", "highlighter", "text", "shape", "image"].map((tool) => ({
    id: `tool.${tool}`,
    title: `Tool: ${tool[0]!.toUpperCase()}${tool.slice(1)}`,
    run: () => app.setTool(tool as Parameters<typeof app.setTool>[0]),
  })),
];

/** Case-insensitive substring filter over title and id. Empty query → all. */
export function filterCommands(query: string): CommandDef[] {
  const q = query.trim().toLowerCase();
  const visible = COMMANDS.filter((c) => !c.when || c.when());
  if (!q) return [...visible];
  return visible.filter(
    (c) => c.title.toLowerCase().includes(q) || c.id.toLowerCase().includes(q),
  );
}

/** Finds the first command bound to a shortcut spec. */
export function commandForShortcut(spec: string): CommandDef | undefined {
  return COMMANDS.find((c) => c.shortcut === spec && (!c.when || c.when()));
}
