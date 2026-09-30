/**
 * Applies the theme to the document root and tracks the system preference
 * when the theme is "system". UI theme and document dark-view are
 * independent; this only touches application chrome.
 */

import { app, type Theme } from "../state/app";

function systemPrefersDark(): boolean {
  return typeof window !== "undefined" && !!window.matchMedia?.("(prefers-color-scheme: dark)").matches;
}

function resolveTheme(theme: Theme): "dark" | "light" {
  if (theme !== "system") return theme;
  return systemPrefersDark() ? "dark" : "light";
}

function apply(theme: Theme): void {
  document.documentElement.dataset.theme = resolveTheme(theme);
}

export function initTheme(): void {
  apply(app.get().theme);
  app.subscribe((s) => apply(s.theme));

  // Follow OS changes while in system mode
  window.matchMedia?.("(prefers-color-scheme: dark)").addEventListener?.("change", () => {
    if (app.get().theme === "system") apply("system");
  });
}
