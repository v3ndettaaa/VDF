/**
 * Command palette (Ctrl+K): filter + execute the command registry.
 * Small overlay listbox with keyboard navigation.
 */

import { app } from "../state/app";
import { COMMANDS, filterCommands, type CommandDef } from "../state/commands";

let instance: { root: HTMLElement; input: HTMLInputElement; list: HTMLUListElement } | null = null;
let visible = false;
let filtered: CommandDef[] = [];
let selected = 0;

export function openPalette(): void {
  if (!instance) build();
  visible = true;
  const { root, input, list } = instance!;
  root.style.display = "flex";
  resetSelection();
  renderList();
  input.value = "";
  input.focus();
  list.setAttribute("aria-activedescendant", activeId());
}

export function closePalette(): void {
  if (!instance || !visible) return;
  visible = false;
  instance.root.style.display = "none";
  app.setStatus("Ready");
}

export function isPaletteOpen(): boolean {
  return visible;
}

function activeId(): string {
  return `palette-item-${selected}`;
}

function resetSelection(): void {
  filtered = COMMANDS;
  selected = 0;
}

function renderList(): void {
  const { list } = instance!;
  list.textContent = "";
  filtered = filterCommands(instance!.input.value);
  selected = Math.min(selected, Math.max(0, filtered.length - 1));

  if (filtered.length === 0) {
    const empty = document.createElement("li");
    empty.className = "palette-empty";
    empty.textContent = "No matching commands";
    list.appendChild(empty);
    return;
  }

  filtered.forEach((cmd, i) => {
    const li = document.createElement("li");
    li.className = "palette-item";
    li.id = `palette-item-${i}`;
    li.setAttribute("role", "option");
    li.setAttribute("aria-selected", String(i === selected));

    const title = document.createElement("span");
    title.className = "palette-title";
    title.textContent = cmd.title;
    li.appendChild(title);

    if (cmd.shortcut) {
      const kbd = document.createElement("span");
      kbd.className = "palette-kbd";
      kbd.textContent = cmd.shortcut;
      li.appendChild(kbd);
    }

    li.addEventListener("click", () => execute(cmd));
    li.addEventListener("mousemove", () => {
      if (selected !== i) {
        selected = i;
        markSelected();
      }
    });
    list.appendChild(li);
  });
  markSelected();
}

function markSelected(): void {
  const { list } = instance!;
  [...list.children].forEach((el, i) => {
    el.setAttribute("aria-selected", String(i === selected));
    if (i === selected) el.scrollIntoView({ block: "nearest" });
  });
  list.setAttribute("aria-activedescendant", activeId());
}

async function execute(cmd: CommandDef): Promise<void> {
  closePalette();
  try {
    await cmd.run();
  } catch (err) {
    app.setStatus(`Command failed: ${String(err)}`);
  }
}

function build(): void {
  const root = document.createElement("div");
  root.className = "overlay";
  root.style.display = "none";

  const box = document.createElement("div");
  box.className = "palette";
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-label", "Command palette");

  const input = document.createElement("input");
  input.className = "palette-input";
  input.placeholder = "Type a command…";
  input.setAttribute("aria-label", "Filter commands");
  input.addEventListener("input", () => {
    selected = 0;
    renderList();
  });
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      selected = Math.min(selected + 1, filtered.length - 1);
      markSelected();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selected = Math.max(selected - 1, 0);
      markSelected();
    } else if (e.key === "Enter") {
      e.preventDefault();
      const cmd = filtered[selected];
      if (cmd) void execute(cmd);
    } else if (e.key === "Escape") {
      e.preventDefault();
      closePalette();
    }
  });

  const list = document.createElement("ul");
  list.className = "palette-list";
  list.setAttribute("role", "listbox");
  list.setAttribute("aria-label", "Commands");

  root.addEventListener("mousedown", (e) => {
    if (e.target === root) closePalette();
  });

  box.replaceChildren(input, list);
  root.appendChild(box);
  document.getElementById("palette-root")?.appendChild(root);
  instance = { root, input, list };
}
