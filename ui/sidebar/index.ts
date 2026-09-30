/**
 * Sidebar: Pages (thumbnails), Outline, Search, Layers, Properties.
 * Pages and Outline are live in M1; the rest stay honest placeholders
 * until their milestones deliver them (Search → M4, Layers/Properties → M3).
 */

import {
  getOutline,
  requestThumbnails,
  thumbUrl,
  type OutlineNode,
} from "../app/ipc";
import { app } from "../state/app";
import { formatBytes } from "../state/commands";

const THUMB_W = 96;

export function initSidebar(root: HTMLElement): void {
  const tabsBar = document.createElement("div");
  tabsBar.className = "sidebar-tabs";
  tabsBar.setAttribute("role", "tablist");

  const panel = document.createElement("div");
  panel.className = "sidebar-panel";
  panel.setAttribute("role", "tabpanel");

  const panels: Record<string, (el: HTMLElement) => void> = {
    pages: buildPagesPanel,
    outline: buildOutlinePanel,
    search: placeholderPanel("Search", "Full-text search and OCR arrive in M4."),
    layers: placeholderPanel("Layers", "Layer management arrives with editing (M3)."),
    properties: placeholderPanel("Properties", "Object properties arrive with editing (M3)."),
  };

  const tabButtons = new Map<string, HTMLButtonElement>();
  for (const id of Object.keys(panels)) {
    const btn = document.createElement("button");
    btn.className = "sidebar-tab";
    btn.setAttribute("role", "tab");
    btn.textContent = id[0]!.toUpperCase() + id.slice(1);
    btn.addEventListener("click", () => app.setSidebarPanel(id as "pages"));
    tabButtons.set(id, btn);
    tabsBar.appendChild(btn);
  }

  let current = "";
  const renderPanel = (id: string): void => {
    if (current === id) return;
    current = id;
    panel.textContent = "";
    panel.dataset.panel = id;
    panels[id]!(panel);
  };

  const render = (): void => {
    const s = app.get();
    root.dataset.open = String(s.sidebarOpen);
    for (const [id, btn] of tabButtons) {
      btn.setAttribute("aria-selected", String(id === s.sidebarPanel));
    }
    renderPanel(s.sidebarPanel);
  };

  root.replaceChildren(tabsBar, panel);
  app.subscribe(render);
  render();
}

function placeholderPanel(title: string, note: string) {
  return (panel: HTMLElement): void => {
    const box = document.createElement("div");
    box.className = "sidebar-placeholder";
    const strong = document.createElement("strong");
    strong.textContent = title;
    box.appendChild(strong);
    box.appendChild(document.createTextNode(note));
    panel.appendChild(box);
  };
}

// ---- Pages panel ----

function buildPagesPanel(panel: HTMLElement): void {
  const grid = document.createElement("div");
  grid.className = "pages-grid";
  panel.appendChild(grid);

  const cells = new Map<number, { box: HTMLDivElement; canvas: HTMLCanvasElement; drawn: boolean }>();
  let renderedFor: number | null = null;

  const build = (): void => {
    const s = app.get();
    const doc = s.activeDoc;
    if (!doc) {
      grid.textContent = "";
      const note = document.createElement("div");
      note.className = "sidebar-placeholder";
      note.textContent = "No document open.";
      grid.appendChild(note);
      renderedFor = null;
      return;
    }
    if (renderedFor === doc.id) return;
    renderedFor = doc.id;
    grid.textContent = "";
    cells.clear();
    for (let i = 0; i < doc.pageCount; i++) {
      const box = document.createElement("div");
      box.className = "page-cell";
      box.title = `Page ${i + 1}`;
      const canvas = document.createElement("canvas");
      canvas.width = THUMB_W;
      canvas.height = Math.round((THUMB_W * 792) / 612); // A4-ish default
      canvas.dataset.page = String(i);
      const label = document.createElement("div");
      label.className = "page-label";
      label.textContent = String(i + 1);
      box.append(canvas, label);
      box.addEventListener("click", () => {
        void import("../viewport").then((m) => m.controller?.gotoPage(i));
      });
      grid.appendChild(box);
      cells.set(i, { box, canvas, drawn: false });
    }
  };

  const pump = (): void => {
    const s = app.get();
    const doc = s.activeDoc;
    if (!doc || renderedFor !== doc.id) {
      build();
      return;
    }
    const active = s.viewportStats?.activePage ?? 0;
    const from = Math.max(0, active - 6);
    const to = Math.min(doc.pageCount - 1, active + 14);
    const want: number[] = [];
    for (let p = from; p <= to; p++) {
      const cell = cells.get(p);
      if (cell && !cell.drawn) want.push(p);
    }
    if (want.length > 0) void requestThumbnails(doc.id, want, THUMB_W);
    for (const p of want) {
      const cell = cells.get(p)!;
      void fetch(thumbUrl(doc.id, p, THUMB_W))
        .then(async (resp) => {
          if (!resp.ok) return; // not ready yet; retried on next pump
          const buf = await resp.arrayBuffer();
          if (buf.byteLength < 8) return;
          drawRgba(cell.canvas, buf);
          cell.drawn = true;
          cell.box.classList.add("ready");
        })
        .catch(() => undefined);
    }
    // highlight active page
    for (const [p, cell] of cells) {
      cell.box.classList.toggle("active", p === active);
    }
  };

  app.subscribe(pump);
  pump();
}

/** Draws raw RGBA bytes (from vdf-thumb://) into a canvas. */
function drawRgba(canvas: HTMLCanvasElement, buf: ArrayBuffer): void {
  const view = new DataView(buf);
  // thumbnails carry no header: width/height come from the poll's
  // thumbs_ready — but for the simple panel we parse a tiny 8-byte trailer
  // format: [w u32 LE][h u32 LE][RGBA...]
  const w = view.getUint32(0, true);
  const h = view.getUint32(4, true);
  if (w === 0 || h === 0 || 8 + w * h * 4 > buf.byteLength) return;
  const rgba = new Uint8ClampedArray(buf, 8, w * h * 4);
  const imageData = new ImageData(rgba, w, h);
  canvas.width = w;
  canvas.height = h;
  canvas.getContext("2d")?.putImageData(imageData, 0, 0);
}

// ---- Outline panel ----

function buildOutlinePanel(panel: HTMLElement): void {
  const note = document.createElement("div");
  note.className = "sidebar-placeholder";
  note.textContent = "No outline loaded yet.";
  panel.appendChild(note);

  const load = async (): Promise<void> => {
    const doc = app.get().activeDoc;
    if (!doc) {
      note.textContent = "No document open.";
      return;
    }
    try {
      const tree = await getOutline(doc.id);
      panel.textContent = "";
      if (tree.length === 0) {
        const empty = document.createElement("div");
        empty.className = "sidebar-placeholder";
        empty.textContent = "This document has no outline/bookmarks.";
        panel.appendChild(empty);
        return;
      }
      const list = document.createElement("ul");
      list.className = "outline-list";
      appendNodes(list, tree, 0);
      panel.appendChild(list);
    } catch (err) {
      note.textContent = `Outline unavailable: ${String(err)}`;
    }
  };

  app.subscribe((s) => {
    if (!s.activeDoc) {
      panel.textContent = "";
      note.textContent = "No document open.";
      panel.appendChild(note);
    }
  });
  void load();
}

function appendNodes(list: HTMLUListElement, nodes: OutlineNode[], depth: number): void {
  for (const node of nodes) {
    const li = document.createElement("li");
    li.style.paddingLeft = `${depth * 12}px`;
    const btn = document.createElement("button");
    btn.className = "outline-item";
    btn.textContent = node.title || "(untitled)";
    btn.title = node.page !== null ? `Go to page ${node.page + 1}` : node.title;
    btn.addEventListener("click", () => {
      if (node.page !== null) {
        void import("../viewport").then((m) => m.controller?.gotoPage(node.page!));
      }
    });
    li.appendChild(btn);
    list.appendChild(li);
    if (node.children.length > 0) {
      appendNodes(list, node.children, depth + 1);
    }
  }
}

// keep formatBytes referenced (statusbar owns it too)
void formatBytes;
