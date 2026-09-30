/**
 * Document viewport.
 *
 * M0: empty-state plus the File→Open demonstration (picked file name/size).
 * The WebGL2 compositor canvas is already mounted because it is the hot-path
 * surface M1 renders into — but nothing draws here yet, by design.
 */

import { app } from "../state/app";
import { formatBytes } from "../state/commands";

export function initViewport(root: HTMLElement): void {
  const canvas = document.createElement("canvas");
  canvas.className = "viewport-canvas";
  canvas.setAttribute("aria-hidden", "true");

  const empty = document.createElement("div");
  empty.className = "viewport-empty";

  const title = document.createElement("div");
  title.className = "title";
  title.textContent = "Open a document to begin";

  const hint = document.createElement("div");
  const kbd = document.createElement("kbd");
  kbd.textContent = "Ctrl+O";
  hint.append(kbd, document.createTextNode(" to open a PDF — or press "));
  const kbd2 = document.createElement("kbd");
  kbd2.textContent = "Ctrl+K";
  hint.appendChild(kbd2);

  const fileInfo = document.createElement("div");
  fileInfo.className = "viewport-file-info";

  empty.append(title, hint, fileInfo);
  root.replaceChildren(canvas, empty);

  const render = (): void => {
    const s = app.get();
    const f = s.lastOpenedFile;
    fileInfo.textContent = f ? `${f.name} — ${formatBytes(f.size_bytes)}` : "";
    empty.style.display = f ? "none" : "flex";
  };

  app.subscribe(render);
  render();
}
