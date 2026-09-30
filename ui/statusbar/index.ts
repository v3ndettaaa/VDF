/**
 * Minimal status/zoom bar: status message, file info, page indicator,
 * cache stats, and zoom controls (mirror of the Rust zoom controller).
 */

import { app, ZOOM_MAX_PERCENT, ZOOM_MIN_PERCENT } from "../state/app";

export function initStatusbar(root: HTMLElement): void {
  const left = document.createElement("div");
  left.className = "statusbar-left";
  const right = document.createElement("div");
  right.className = "statusbar-right";

  const message = document.createElement("span");
  message.className = "status-message";
  message.setAttribute("role", "status");
  left.appendChild(message);

  const file = document.createElement("span");
  file.className = "viewport-file-info";
  left.appendChild(file);

  const pageInfo = document.createElement("span");
  pageInfo.className = "viewport-file-info";
  left.appendChild(pageInfo);

  const diag = document.createElement("span");
  diag.className = "viewport-file-info";
  diag.title = "tile cache / rendered tiles / stale dropped";
  left.appendChild(diag);

  const zoomOut = document.createElement("button");
  zoomOut.className = "zoom-btn";
  zoomOut.textContent = "−";
  zoomOut.setAttribute("aria-label", "Zoom out");
  zoomOut.addEventListener("click", () => {
    void import("../state/commands").then((m) => m.COMMANDS.find((c) => c.id === "zoom.out")?.run());
  });

  const zoomValue = document.createElement("button");
  zoomValue.className = "zoom-value";
  zoomValue.title = "Reset zoom to 100%";
  zoomValue.addEventListener("click", () => {
    void import("../state/commands").then((m) => m.COMMANDS.find((c) => c.id === "zoom.reset")?.run());
  });

  const zoomIn = document.createElement("button");
  zoomIn.className = "zoom-btn";
  zoomIn.textContent = "+";
  zoomIn.setAttribute("aria-label", "Zoom in");
  zoomIn.addEventListener("click", () => {
    void import("../state/commands").then((m) => m.COMMANDS.find((c) => c.id === "zoom.in")?.run());
  });

  right.append(zoomOut, zoomValue, zoomIn);

  const render = (): void => {
    const s = app.get();
    message.textContent = s.statusMessage;
    const f = s.lastOpenedFile;
    file.textContent = f ? `${f.name} · ${f.size_bytes.toLocaleString()} bytes` : "";
    const doc = s.activeDoc;
    const page = (s.viewportStats?.activePage ?? 0) + 1;
    pageInfo.textContent = doc ? `Page ${page} / ${doc.pageCount}` : "";
    const stats = s.viewportStats;
    diag.textContent = stats
      ? `tiles ${stats.cacheTiles} · ${(stats.cacheBytes / (1024 * 1024)).toFixed(1)} MiB · stale ${stats.staleDropped}`
      : "";
    zoomValue.textContent = `${Math.round(s.zoom)}%`;
    zoomIn.disabled = s.zoom >= ZOOM_MAX_PERCENT;
    zoomOut.disabled = s.zoom <= ZOOM_MIN_PERCENT;
  };

  root.replaceChildren(left, right);
  app.subscribe(render);
  render();
}
