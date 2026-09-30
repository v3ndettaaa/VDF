import { describe, expect, it } from "vitest";
import { app, AppStore, SIDEBAR_PANELS, ZOOM_MAX_PERCENT, ZOOM_MIN_PERCENT } from "./state/app";
import { filterCommands, commandForShortcut, formatBytes } from "./state/commands";
import { Store } from "./state/store";

describe("Store", () => {
  it("notifies subscribers on set with the new snapshot", () => {
    const store = new Store<{ n: number }>({ n: 1 });
    const seen: number[] = [];
    const unsub = store.subscribe((s) => seen.push(s.n));
    store.set({ n: 2 });
    store.set({ n: 3 });
    unsub();
    store.set({ n: 4 });
    expect(seen).toEqual([2, 3]);
    expect(store.get().n).toBe(4);
  });

  it("update derives the patch from current state", () => {
    const store = new Store<{ n: number }>({ n: 5 });
    store.update((s) => ({ n: s.n * 2 }));
    expect(store.get().n).toBe(10);
  });
});

describe("AppStore tabs", () => {
  it("opens the first document as an active tab", () => {
    const s = new AppStore();
    const id = s.openDocumentTab("book.pdf");
    expect(s.get().tabs).toHaveLength(1);
    expect(s.get().activeTabId).toBe(id);
  });

  it("focuses the existing tab instead of duplicating it", () => {
    const s = new AppStore();
    const a = s.openDocumentTab("book.pdf");
    const b = s.openDocumentTab("book.pdf");
    expect(a).toBe(b);
    expect(s.get().tabs).toHaveLength(1);
  });

  it("closes a tab and activates a sensible neighbor", () => {
    const s = new AppStore();
    const a = s.openDocumentTab("a.pdf");
    const b = s.openDocumentTab("b.pdf");
    const c = s.openDocumentTab("c.pdf");
    expect(s.get().activeTabId).toBe(c);
    s.closeTab(c);
    expect(s.get().activeTabId).toBe(b);
    s.closeTab(b);
    expect(s.get().activeTabId).toBe(a);
    s.closeTab(a);
    expect(s.get().tabs).toHaveLength(0);
    expect(s.get().activeTabId).toBeNull();
  });

  it("ignores close of unknown tab", () => {
    const s = new AppStore();
    s.openDocumentTab("a.pdf");
    expect(s.closeTab("nope")).toBe(false);
    expect(s.get().tabs).toHaveLength(1);
  });
});

describe("AppStore zoom", () => {
  it("clamps to the zoom limits", () => {
    const s = new AppStore();
    s.setZoom(99999);
    expect(s.get().zoom).toBe(ZOOM_MAX_PERCENT);
    s.setZoom(0);
    expect(s.get().zoom).toBe(ZOOM_MIN_PERCENT);
    s.setZoom(150.555);
    expect(s.get().zoom).toBe(150.6);
  });

  it("zoomBy multiplies and reset restores", () => {
    const s = new AppStore();
    s.zoomBy(2);
    expect(s.get().zoom).toBe(200);
    s.resetZoom();
    expect(s.get().zoom).toBe(100);
    s.setZoom(Number.NaN);
    expect(s.get().zoom).toBe(100);
  });
});

describe("AppStore theme/tool/sidebar", () => {
  it("cycles dark → light → system → dark", () => {
    const s = new AppStore();
    s.setTheme("dark");
    expect(s.cycleTheme()).toBe("light");
    expect(s.cycleTheme()).toBe("system");
    expect(s.cycleTheme()).toBe("dark");
  });

  it("tracks active tool", () => {
    const s = new AppStore();
    s.setTool("pen");
    expect(s.get().activeTool).toBe("pen");
  });

  it("setSidebarPanel opens the sidebar", () => {
    const s = new AppStore();
    expect(s.get().sidebarOpen).toBe(false);
    s.setSidebarPanel("search");
    expect(s.get().sidebarOpen).toBe(true);
    expect(s.get().sidebarPanel).toBe("search");
    expect(SIDEBAR_PANELS.map((p) => p.id)).toContain("search");
  });
});

describe("command registry", () => {
  it("empty filter returns the shell commands", () => {
    expect(filterCommands("").length).toBeGreaterThanOrEqual(10);
  });

  it("filters case-insensitively over title and id", () => {
    expect(filterCommands("TOOL.PEN").some((c) => c.id === "tool.pen")).toBe(true);
    expect(filterCommands("zzz-no-such-command")).toHaveLength(0);
  });

  it("reveals viewer commands only while a document is open", () => {
    // no doc open: zoom commands are hidden
    expect(filterCommands("zoom").some((c) => c.id === "zoom.in")).toBe(false);
    const store = new AppStore();
    store.setActiveDoc({ id: 1, name: "a.pdf", pageCount: 3 });
    // simulate for the module-level store used by commands.ts
    app.setActiveDoc({ id: 1, name: "a.pdf", pageCount: 3 });
    expect(filterCommands("zoom").some((c) => c.id === "zoom.in")).toBe(true);
    expect(commandForShortcut("Ctrl+1")?.id).toBe("zoom.fitWidth");
    app.setActiveDoc(null);
  });

  it("binds known shortcuts", () => {
    expect(commandForShortcut("Ctrl+K")?.id).toBe("view.commandPalette");
    expect(commandForShortcut("Ctrl+O")?.id).toBe("file.open");
    expect(commandForShortcut("Ctrl+Shift+T")?.id).toBe("view.theme.cycle");
    expect(commandForShortcut("Ctrl+Q")).toBeUndefined();
  });
});

describe("formatBytes", () => {
  it("formats across magnitudes", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2.0 KiB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5.0 MiB");
  });
});
