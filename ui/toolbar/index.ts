/**
 * Contextual toolbar: primary tools, icon-first. Owns #toolbar only.
 * The tool shelf (advanced tools) arrives in M6.
 */

import { app, TOOLS } from "../state/app";

const ICONS: Record<string, string> = {
  select: `<path d="M5 3l14 8-6 1.5L15.5 19 13 20l-2.5-6.5L5 17z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/>`,
  pen: `<path d="M4 20l1-4L16.5 4.5a2.1 2.1 0 013 3L8 19l-4 1z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/>`,
  eraser: `<path d="M8 20h11M5.5 16.5l8-8 5 5-6 6h-4l-3-3z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/>`,
  highlighter: `<path d="M6 20h12M4.5 15.5l9.5-9.5 4 4-8 8h-4l-1.5-2.5z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/>`,
  text: `<path d="M5 6h14M12 6v13M9 19h6" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>`,
  shape: `<rect x="4.5" y="4.5" width="15" height="15" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.6"/>`,
  image: `<rect x="3.5" y="5" width="17" height="14" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.6"/><circle cx="9" cy="10" r="1.6" fill="currentColor"/><path d="M4.5 17l5-5 3.5 3.5 3-3 4.5 4.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/>`,
  sidebar: `<rect x="3.5" y="4.5" width="17" height="15" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M10 4.5v15" stroke="currentColor" stroke-width="1.6"/>`,
  palette: `<circle cx="6" cy="7" r="1.3" fill="currentColor"/><circle cx="12" cy="5.5" r="1.3" fill="currentColor"/><circle cx="18" cy="7" r="1.3" fill="currentColor"/><path d="M4 12a8 8 0 1016 0c0-1.5-1-2.5-2.5-2.5h-11C5 9.5 4 10.5 4 12z" fill="none" stroke="currentColor" stroke-width="1.6"/>`,
};

function svg(name: string): SVGSVGElement {
  const el = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  el.setAttribute("viewBox", "0 0 24 24");
  el.setAttribute("aria-hidden", "true");
  el.innerHTML = ICONS[name] ?? "";
  return el;
}

export function initToolbar(root: HTMLElement): void {
  root.setAttribute("role", "toolbar");

  const toolButtons = new Map<string, HTMLButtonElement>();
  const sidebarBtn = document.createElement("button");
  const paletteBtn = document.createElement("button");

  const render = (): void => {
    const s = app.get();
    for (const [id, btn] of toolButtons) {
      btn.setAttribute("aria-pressed", String(id === s.activeTool));
      btn.title = TOOLS.find((t) => t.id === id)?.title ?? id;
    }
    sidebarBtn.setAttribute("aria-pressed", String(s.sidebarOpen));
  };

  for (const tool of TOOLS) {
    const btn = document.createElement("button");
    btn.className = "tool-btn";
    btn.dataset.tool = tool.id;
    btn.setAttribute("aria-label", tool.title);
    btn.appendChild(svg(tool.id));
    btn.addEventListener("click", () => app.setTool(tool.id));
    toolButtons.set(tool.id, btn);
    root.appendChild(btn);
  }

  root.appendChild(sep());
  sidebarBtn.className = "tool-btn";
  sidebarBtn.setAttribute("aria-label", "Toggle sidebar");
  sidebarBtn.appendChild(svg("sidebar"));
  sidebarBtn.addEventListener("click", () => app.toggleSidebar());
  root.appendChild(sidebarBtn);

  paletteBtn.className = "tool-btn";
  paletteBtn.setAttribute("aria-label", "Command palette");
  paletteBtn.title = "Command palette (Ctrl+K)";
  paletteBtn.appendChild(svg("palette"));
  paletteBtn.addEventListener("click", () => {
    void import("../command-palette").then((m) => m.openPalette());
  });
  root.appendChild(paletteBtn);

  app.subscribe(render);
  render();
}

function sep(): HTMLDivElement {
  const el = document.createElement("div");
  el.className = "toolbar-sep";
  el.setAttribute("aria-hidden", "true");
  return el;
}
