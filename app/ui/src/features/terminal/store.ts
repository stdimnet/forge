/** Terminal tabs (one shell session each), like PHPStorm's "Local", "Local (2)", ... */
import { createSignal } from "solid-js";
import { getSetting } from "@/state/settings";
import { isToolWindowVisible, toggleToolWindow } from "@/state/layout";
import { workspace } from "@/state/workspace";
import type { SshTarget } from "@/ipc/terminal";
import type { Progress } from "./osc";

export interface TerminalTab {
  /** UI id, also used as the backend session id. */
  id: string;
  title: string;
  /** Renamed by the user: keep the title. */
  renamed?: boolean;
  /** Start directory relative to the project root (empty = default). */
  cwd?: string;
  /** Title set by the running program (OSC 0 / OSC 2). */
  appTitle?: string;
  /** Progress reported by the running program (OSC 9;4). */
  progress?: Progress;
  /** The program rang the bell while the tab was not visible. */
  bell?: boolean;
  /** Current directory reported by the shell (OSC 7), absolute. */
  liveCwd?: string;
  /** Run this program instead of the shell (e.g. `docker exec -it …` from Services). */
  command?: TerminalProgram;
  /** An SSH session to another host (Tools | Start SSH Session…). */
  ssh?: SshTarget;
}

export interface TerminalProgram {
  program: string;
  args: string[];
  env?: [string, string][];
  /** Run on this machine even in a remote project (Docker or kubectl of this machine). */
  local?: boolean;
}

/** A shell the terminal's "+" menu offers besides the default one (e.g. "WSL: Ubuntu"). */
export interface ShellProfile {
  label: string;
  command: TerminalProgram & { title?: string };
  /** Start directory (absolute; default: the project root). */
  cwd?: string;
}

const [profileSources, setProfileSources] = createSignal<(() => ShellProfile[])[]>([]);

/** Add shells to the "+" menu; `profiles` is reactive. */
export function registerShellProfiles(profiles: () => ShellProfile[]): void {
  setProfileSources((s) => [...s, profiles]);
}

/** The extra shells of the "+" menu. */
export const shellProfiles = (): ShellProfile[] => profileSources().flatMap((p) => p());

const [tabs, setTabs] = createSignal<TerminalTab[]>([]);
const [activeTab, setActiveTabSignal] = createSignal<string | null>(null);
const [renaming, setRenaming] = createSignal<string | null>(null);

export { tabs, activeTab, renaming, setRenaming };

/**
 * Tab ids in order. Tabs are replaced on every update (title, progress), so lists render by
 * id; rendering by tab object would recreate a tab's DOM, including its terminal host.
 */
export const tabIds = () => tabs().map((t) => t.id);
export const tabById = (id: string) => tabs().find((t) => t.id === id);

let seq = 0;

/** Activate a tab; this also clears its bell marker. */
export function setActiveTab(id: string | null): void {
  setActiveTabSignal(id);
  if (id && tabs().some((t) => t.id === id && t.bell)) updateTab(id, { bell: false });
}

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
    command: command && { program: command.program, args: command.args, env: command.env, local: command.local },
  };
  setTabs((t) => [...t, tab]);
  setActiveTab(tab.id);
  return tab;
}

/** A tab with an SSH session, named after the host ("shop.example.com", "shop.example.com (2)"). */
export function newSshTab(title: string, ssh: SshTarget): TerminalTab {
  const tab: TerminalTab = {
    id: `term-${Date.now().toString(36)}-${++seq}`,
    title: nextTitle(
      tabs().map((t) => t.title),
      title,
    ),
    ssh,
  };
  setTabs((t) => [...t, tab]);
  setActiveTab(tab.id);
  return tab;
}

/** New tab in the directory the active tab's shell is in (OSC 7), like kitty's new_tab_with_cwd. */
export function newTabHere(): TerminalTab {
  const cur = tabs().find((t) => t.id === activeTab());
  return newTab(cur?.liveCwd ?? cur?.cwd);
}

export function updateTab(id: string, patch: Partial<TerminalTab>): void {
  setTabs((list) => list.map((t) => (t.id === id ? { ...t, ...patch } : t)));
}

export function closeTab(id: string): void {
  const list = tabs();
  const idx = list.findIndex((t) => t.id === id);
  if (idx < 0) return;
  const rest = list.filter((t) => t.id !== id);
  setTabs(rest);
  if (activeTab() === id) setActiveTab(rest[Math.min(idx, rest.length - 1)]?.id ?? null);
  // Like PHPStorm, closing the last session hides the tool window.
  if (!rest.length && isToolWindowVisible("terminal")) toggleToolWindow("terminal");
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

/**
 * The name shown on a tab: the user's name if renamed, else the program's title (PHPStorm's
 * "Show application title"), else "Local (n)".
 */
export function tabLabel(tab: TerminalTab, showAppTitle = true): string {
  if (tab.renamed || !showAppTitle) return tab.title;
  return tab.appTitle?.trim() || tab.title;
}

export const displayTitle = (tab: TerminalTab) =>
  tabLabel(tab, getSetting<boolean>("terminal.showApplicationTitle", true));

/** Tab whose progress the status bar shows: the active tab if it reports one, else the first. */
export function progressTab(list: TerminalTab[], active: string | null): TerminalTab | undefined {
  return list.find((t) => t.id === active && t.progress) ?? list.find((t) => t.progress);
}

/** Combined progress of all tabs for the stripe icon: errors win, then paused, then the slowest. */
export function overallProgress(list: TerminalTab[]): Progress | undefined {
  const all = list.map((t) => t.progress).filter((p): p is Progress => !!p);
  if (!all.length) return undefined;
  for (const state of ["error", "paused"] as const) {
    const p = all.find((x) => x.state === state);
    if (p) return p;
  }
  const determinate = all.filter((p) => p.state === "normal");
  if (!determinate.length) return all[0];
  return { state: "normal", percent: Math.min(...determinate.map((p) => p.percent)) };
}
