/**
 * Document tabs. Owns #tabs only; talks to the store.
 */

import { app } from "../state/app";
import { openPdfFlow } from "../state/commands";

export function initTabs(root: HTMLElement): void {
  root.setAttribute("role", "tablist");

  const render = (): void => {
    const s = app.get();
    root.textContent = "";

    for (const tab of s.tabs) {
      const el = document.createElement("div");
      el.className = "tab";
      el.setAttribute("role", "tab");
      el.setAttribute("aria-selected", String(tab.id === s.activeTabId));
      el.title = tab.title;

      const title = document.createElement("span");
      title.className = "tab-title";
      title.textContent = tab.title;
      el.appendChild(title);

      const close = document.createElement("button");
      close.className = "tab-close";
      close.setAttribute("aria-label", `Close ${tab.title}`);
      close.textContent = "×";
      close.addEventListener("click", (e) => {
        e.stopPropagation();
        app.closeTab(tab.id);
      });
      el.appendChild(close);

      el.addEventListener("click", () => app.set({ activeTabId: tab.id }));
      root.appendChild(el);
    }

    const newBtn = document.createElement("button");
    newBtn.className = "tabs-new";
    newBtn.setAttribute("aria-label", "Open document");
    newBtn.title = "Open document (Ctrl+O)";
    newBtn.textContent = "+";
    newBtn.addEventListener("click", () => void openPdfFlow());
    root.appendChild(newBtn);
  };

  app.subscribe(render);
  render();
}
