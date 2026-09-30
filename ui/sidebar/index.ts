/**
 * Sidebar with module tabs (Pages, Outline, Search, Layers, Properties).
 * Panels are honest placeholders until their milestones deliver them:
 * Pages/Outline → M1, Search → M4, Layers/Properties → M3.
 */

import { app, SIDEBAR_PANELS } from "../state/app";

const PANEL_NOTES: Record<string, { title: string; note: string }> = {
  pages: { title: "Pages", note: "Page thumbnails arrive with the PDF viewer (M1)." },
  outline: { title: "Outline", note: "Document outline/bookmarks arrive with the PDF viewer (M1)." },
  search: { title: "Search", note: "Full-text search and OCR arrive in M4." },
  layers: { title: "Layers", note: "Layer management arrives with editing (M3)." },
  properties: { title: "Properties", note: "Object properties arrive with editing (M3)." },
};

export function initSidebar(root: HTMLElement): void {
  const tabsBar = document.createElement("div");
  tabsBar.className = "sidebar-tabs";
  tabsBar.setAttribute("role", "tablist");

  const panel = document.createElement("div");
  panel.className = "sidebar-panel";
  panel.setAttribute("role", "tabpanel");

  const tabButtons = new Map<string, HTMLButtonElement>();

  for (const p of SIDEBAR_PANELS) {
    const btn = document.createElement("button");
    btn.className = "sidebar-tab";
    btn.setAttribute("role", "tab");
    btn.textContent = p.title;
    btn.addEventListener("click", () => app.setSidebarPanel(p.id));
    tabButtons.set(p.id, btn);
    tabsBar.appendChild(btn);
  }

  const renderPanel = (id: string): void => {
    const note = PANEL_NOTES[id]!;
    panel.textContent = "";
    const box = document.createElement("div");
    box.className = "sidebar-placeholder";
    const strong = document.createElement("strong");
    strong.textContent = note.title;
    box.appendChild(strong);
    box.appendChild(document.createTextNode(note.note));
    panel.appendChild(box);
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
