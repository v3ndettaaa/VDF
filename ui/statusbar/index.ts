/**
 * Minimal status/zoom bar. Owns #statusbar only.
 * Page indicator and document info grow in M1; zoom percent reflects the
 * central zoom state that the DocumentZoomController (M1, Rust-authoritative)
 * will feed — the store value is the UI mirror.
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

  const zoomOut = document.createElement("button");
  zoomOut.className = "zoom-btn";
  zoomOut.textContent = "−";
  zoomOut.setAttribute("aria-label", "Zoom out");
  zoomOut.addEventListener("click", () => app.zoomBy(1 / 1.1));

  const zoomValue = document.createElement("button");
  zoomValue.className = "zoom-value";
  zoomValue.title = "Reset zoom to 100%";
  zoomValue.addEventListener("click", () => app.resetZoom());

  const zoomIn = document.createElement("button");
  zoomIn.className = "zoom-btn";
  zoomIn.textContent = "+";
  zoomIn.setAttribute("aria-label", "Zoom in");
  zoomIn.addEventListener("click", () => app.zoomBy(1.1));

  right.append(zoomOut, zoomValue, zoomIn);

  const render = (): void => {
    const s = app.get();
    message.textContent = s.statusMessage;
    const f = s.lastOpenedFile;
    file.textContent = f
      ? `${f.name} · ${f.size_bytes.toLocaleString()} bytes`
      : "";
    zoomValue.textContent = `${Math.round(s.zoom)}%`;
    zoomIn.disabled = s.zoom >= ZOOM_MAX_PERCENT;
    zoomOut.disabled = s.zoom <= ZOOM_MIN_PERCENT;
  };

  root.replaceChildren(left, right);
  app.subscribe(render);
  render();
}
