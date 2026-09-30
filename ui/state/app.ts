/**
 * Application state: one store, typed slices, and the actions modules use.
 * Modules never mutate each other's DOM — they act through this store.
 */

import { Store } from "./store";

export type Theme = "dark" | "light" | "system";

export type ToolId = "select" | "pen" | "eraser" | "highlighter" | "text" | "shape" | "image";

export type SidebarPanelId = "pages" | "outline" | "search" | "layers" | "properties";

export interface Tab {
  id: string;
  title: string;
}

export interface FileInfo {
  name: string;
  size_bytes: number;
  extension: string;
}

export interface AppState {
  theme: Theme;
  tabs: Tab[];
  activeTabId: string | null;
  activeTool: ToolId;
  sidebarOpen: boolean;
  sidebarPanel: SidebarPanelId;
  /** Zoom in percent (100 = 100%). */
  zoom: number;
  statusMessage: string;
  lastOpenedFile: FileInfo | null;
}

export const SIDEBAR_PANELS: ReadonlyArray<{ id: SidebarPanelId; title: string }> = [
  { id: "pages", title: "Pages" },
  { id: "outline", title: "Outline" },
  { id: "search", title: "Search" },
  { id: "layers", title: "Layers" },
  { id: "properties", title: "Properties" },
];

export const TOOLS: ReadonlyArray<{ id: ToolId; title: string }> = [
  { id: "select", title: "Select" },
  { id: "pen", title: "Pen" },
  { id: "eraser", title: "Eraser" },
  { id: "highlighter", title: "Highlighter" },
  { id: "text", title: "Text" },
  { id: "shape", title: "Shape" },
  { id: "image", title: "Image" },
];

export const ZOOM_MIN_PERCENT = 5;
export const ZOOM_MAX_PERCENT = 6400;

const THEME_ORDER: Theme[] = ["dark", "light", "system"];

let tabCounter = 0;

function clampZoom(percent: number): number {
  if (!Number.isFinite(percent)) return 100;
  return Math.min(ZOOM_MAX_PERCENT, Math.max(ZOOM_MIN_PERCENT, Math.round(percent * 10) / 10));
}

export class AppStore extends Store<AppState> {
  constructor() {
    super({
      theme: "dark",
      tabs: [],
      activeTabId: null,
      activeTool: "select",
      sidebarOpen: false,
      sidebarPanel: "pages",
      zoom: 100,
      statusMessage: "Ready",
      lastOpenedFile: null,
    });
  }

  setTheme(theme: Theme): void {
    this.set({ theme });
  }

  /** Cycles dark → light → system → dark. */
  cycleTheme(): Theme {
    const current = this.get().theme;
    const next = THEME_ORDER[(THEME_ORDER.indexOf(current) + 1) % THEME_ORDER.length]!;
    this.setTheme(next);
    return next;
  }

  setTool(tool: ToolId): void {
    this.set({ activeTool: tool });
  }

  toggleSidebar(): void {
    this.update((s) => ({ sidebarOpen: !s.sidebarOpen }));
  }

  setSidebarPanel(panel: SidebarPanelId): void {
    this.set({ sidebarPanel: panel, sidebarOpen: true });
  }

  setZoom(percent: number): void {
    this.set({ zoom: clampZoom(percent) });
  }

  zoomBy(factor: number): void {
    this.update((s) => ({ zoom: clampZoom(s.zoom * factor) }));
  }

  resetZoom(): void {
    this.set({ zoom: 100 });
  }

  setStatus(message: string): void {
    this.set({ statusMessage: message });
  }

  /**
   * Registers an opened document as a tab and makes it active.
   * Returns the tab id.
   */
  openDocumentTab(title: string): string {
    const existing = this.get().tabs.find((t) => t.title === title);
    if (existing) {
      this.set({ activeTabId: existing.id });
      return existing.id;
    }
    const tab: Tab = { id: `tab-${++tabCounter}`, title };
    this.update((s) => ({ tabs: [...s.tabs, tab], activeTabId: tab.id }));
    return tab.id;
  }

  /** Closes a tab; activates a sensible neighbor. Returns true if closed. */
  closeTab(id: string): boolean {
    const s = this.get();
    const index = s.tabs.findIndex((t) => t.id === id);
    if (index === -1) return false;
    const tabs = s.tabs.filter((t) => t.id !== id);
    const activeTabId =
      s.activeTabId === id
        ? (tabs[Math.min(index, tabs.length - 1)]?.id ?? null)
        : s.activeTabId;
    this.set({ tabs, activeTabId });
    return true;
  }
}

/** Singleton application store. */
export const app = new AppStore();
