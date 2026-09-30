/**
 * VDF application bootstrap: wire modules to their DOM subtrees, then apply
 * the theme and shortcuts. Each module owns exactly one subtree and talks
 * through the store — this file is wiring, nothing more.
 */

import { app } from "../state/app";
import { initTheme } from "./theme";
import { initTabs } from "../tabs";
import { initToolbar } from "../toolbar";
import { initSidebar } from "../sidebar";
import { initStatusbar } from "../statusbar";
import { initViewport } from "../viewport";
import { initShortcuts } from "../shortcuts";
import { getAppInfo, hasTauriBridge } from "./ipc";

function requireElement<T extends HTMLElement = HTMLElement>(id: string): T {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id} — index.html is malformed`);
  return el as T;
}

function bootstrap(): void {
  initTheme();
  initTabs(requireElement("tabs"));
  initToolbar(requireElement("toolbar"));
  initSidebar(requireElement("sidebar"));
  initViewport(requireElement("viewport"));
  initStatusbar(requireElement("statusbar"));
  initShortcuts();

  if (hasTauriBridge()) {
    getAppInfo()
      .then((info) => {
        app.setStatus(`VDF ${info.version} · ${info.platform}/${info.arch} · ready`);
      })
      .catch((err) => app.setStatus(`Shell info failed: ${String(err)}`));
  } else {
    app.setStatus("VDF UI loaded outside the shell (browser mode) — commands need the VDF app");
  }
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", bootstrap);
} else {
  bootstrap();
}
