/** Terminal tabs (one shell session each), like PHPStorm's "Local", "Local (2)", ... */
import { createSignal } from "solid-js";
import { getSetting } from "@/state/settings";
import { layout, toggleToolWindow } from "@/state/layout";
import { workspace } from "@/state/workspace";

export interface TerminalTab {
  /** UI id, also used as the backend session id. */
  id: string;
  title: string;
  /** Renamed by the user: keep the title. */
  renamed?: boolean;
  /** Start directory relative to the project root (empty = default). */
  cwd?: string;
  /** Run this program instead of the shell (e.g. `docker exec -it …` from Services). */
  command?: TerminalProgram;
}

export interface TerminalProgram {
  program: string;
  args: string[];
  env?: [string, string][];
}

const [tabs, setTabs] = createSignal<TerminalTab[]>([]);
const [activeTab, setActiveTab] = createSignal<string | null>(null);
const [renaming, setRenaming] = createSignal<string | null>(null);

export { tabs, activeTab, setActiveTab, renaming, setRenaming };

let seq = 0;

/** "Local", then "Local (2)", "Local (3)", ... for the next free number. */
export function nextTitle(existing: string[], base: string): string {
  if (!existing.includes(base)) return base;
  for (let i = 2; ; i++) if (!existing.includes(`${base} (${i})`)) return `${base} (${i})`;
}

export function newTab(cwd?: string, command?: TerminalProgram & { title?: string }): TerminalTab {
  const setting = getSetting<string>("terminal.tabName", "Local") || "Local";
  // A remote project's shells run on its host: name them after it, like SSH sessions.
  const host = workspace()?.remote?.host;
  const base = command?.title || (host && setting === "Local" ? host : setting);
  const tab: TerminalTab = {
    id: `term-${Date.now().toString(36)}-${++seq}`,
    title: nextTitle(
      tabs().map((t) => t.title),
      base,
    ),
    cwd,
    command: command && { program: command.program, args: command.args, env: command.env },
  };
  setTabs((t) => [...t, tab]);
  setActiveTab(tab.id);
  return tab;
}

export function closeTab(id: string): void {
  const list = tabs();
  const idx = list.findIndex((t) => t.id === id);
  if (idx < 0) return;
  const rest = list.filter((t) => t.id !== id);
  setTabs(rest);
  if (activeTab() === id) setActiveTab(rest[Math.min(idx, rest.length - 1)]?.id ?? null);
  // Like PHPStorm, closing the last session hides the tool window.
  if (!rest.length && layout.active.bottom === "terminal") toggleToolWindow("bottom", "terminal");
}

export function renameTab(id: string, title: string): void {
  const t = title.trim();
  if (t) setTabs((list) => list.map((x) => (x.id === id ? { ...x, title: t, renamed: true } : x)));
  setRenaming(null);
}

export function closeOtherTabs(id: string): string[] {
  const others = tabs()
    .filter((t) => t.id !== id)
    .map((t) => t.id);
  setTabs((list) => list.filter((t) => t.id === id));
  setActiveTab(id);
  return others;
}
