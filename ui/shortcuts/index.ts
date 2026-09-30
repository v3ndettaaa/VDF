/**
 * Keyboard shortcuts, driven by the command registry's shortcut specs.
 * "Ctrl" in a spec means Ctrl on Linux/Windows and ⌘ on macOS.
 */

import { commandForShortcut } from "../state/commands";
import { closePalette, isPaletteOpen, openPalette } from "../command-palette";

function specFor(event: KeyboardEvent): string | null {
  const mod = event.ctrlKey || event.metaKey;
  if (!mod) return null;
  const raw = event.key;
  if (raw.length === 1) {
    const key = raw.toLowerCase();
    // record Shift only where it is meaningful (letter/number keys)
    const shift = event.shiftKey && /[a-z0-9]/.test(key) ? "Shift+" : "";
    return `Ctrl+${shift}${key.toUpperCase()}`;
  }
  return `Ctrl+${raw}`;
}

export function initShortcuts(): void {
  window.addEventListener("keydown", (e) => {
    // Palette toggle works even while the palette input has focus
    if (specFor(e) === "Ctrl+K") {
      e.preventDefault();
      if (isPaletteOpen()) closePalette();
      else openPalette();
      return;
    }
    if (isPaletteOpen()) return; // palette handles its own keys

    const spec = specFor(e);
    if (!spec) return;
    const cmd = commandForShortcut(spec);
    if (cmd) {
      e.preventDefault();
      void cmd.run();
    }
  });
}
