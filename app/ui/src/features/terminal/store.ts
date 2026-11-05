/** Terminal tabs (one shell session each), like PHPStorm's "Local", "Local (2)", ... */
import { createSignal } from "solid-js";
import { getSetting } from "@/state/settings";
import { isToolWindowVisible, toggleToolWindow } from "@/state/layout";
import { workspace } from "@/state/workspace";
import type { SshTarget, TerminalLayout } from "@/ipc/terminal";
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
  /** A session that already runs instead of a new shell (see {@link TerminalSource}). */
  source?: TerminalSource;
  /** Shared with Code With Me guests (host). */
  shared?: boolean;
  /** Opened with this shell profile of the "+" menu (its label). */
  profile?: string;
}

/**
 * A tab that shows a session started elsewhere instead of starting a shell:
 * - `existing`: a session of this window that is already running (a Code With Me guest
 *   opened it on this machine);
 * - `collab`: one of the Code With Me host's shared terminals in a guest's window. It shows
 *   at the host's size; `readOnly` while the guest may only watch. `ended` says why the tab
 *   no longer shows the host's terminal (the host stopped sharing it); the tab stays, with
 *   its output, until the guest closes it.
 */
export type TerminalSource =
  | { kind: "existing" }
  | { kind: "collab"; terminal: number; readOnly: boolean; cols: number; rows: number; ended?: string };

/** An entry another feature adds to a terminal tab's context menu (Share with Guests). */
export interface TerminalTabAction {
  label: string;
  checked?: boolean;
  run: () => void;
}

const [tabActionSources, setTabActionSources] = createSignal<((tab: TerminalTab) => TerminalTabAction[])[]>([]);

/** Add entries to the context menu of terminal tabs. */
export function registerTerminalTabActions(actions: (tab: TerminalTab) => TerminalTabAction[]): void {
  setTabActionSources((s) => [...s, actions]);
}

export const terminalTabActions = (tab: TerminalTab): TerminalTabAction[] => tabActionSources().flatMap((a) => a(tab));

const [newTabBlock, setNewTabBlock] = createSignal<(() => string | null) | null>(null);

/**
 * Why new shells cannot be opened in this window (a Code With Me guest without Full access
 * to the host's terminal), or null. `reason` is reactive.
 */
export function setNewTabGuard(reason: (() => string | null) | null): void {
  setNewTabBlock(() => reason);
}

export const newTabBlocked = (): string | null => newTabBlock()?.() ?? null;

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

export function newTab(cwd?: string, command?: TerminalProgram & { title?: string }, profile?: string): TerminalTab {
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
    ...(profile ? { profile } : {}),
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

/**
 * A tab for a session started elsewhere (see {@link TerminalSource}); not activated unless
 * asked. Returns the existing tab when it is shown already.
 */
export function addSourceTab(id: string, title: string, source: TerminalSource, activate = false): TerminalTab {
  const existing = tabs().find((t) => t.id === id);
  if (existing) return existing;
  const tab: TerminalTab = { id, title: nextTitle(tabs().map((t) => t.title), title), source };
  setTabs((t) => [...t, tab]);
  if (activate || !activeTab()) setActiveTab(tab.id);
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

// ---- Per project ----------------------------------------------------------------------

const [restoringTabs, setRestoringTabs] = createSignal(false);

/** The project's tabs are being restored: the Terminal does not open a fresh tab meanwhile. */
export { restoringTabs, setRestoringTabs };

/** Replace every tab (the opened project's restored layout). */
export function replaceTabs(list: TerminalTab[], active: string | null): void {
  setTabs(list);
  setActiveTabSignal(active && list.some((t) => t.id === active) ? active : (list[0]?.id ?? null));
}

/** `liveCwd` (absolute, from the shell) relative to `root` when it is inside it. */
function startDir(tab: TerminalTab, root: string): string | undefined {
  const live = tab.liveCwd?.replace(/[\\/]+$/, "");
  const base = root.replace(/[\\/]+$/, "");
  if (!live) return tab.cwd || undefined;
  if (live === base) return undefined;
  if (live.startsWith(`${base}/`) || live.startsWith(`${base}\\`)) return live.slice(base.length + 1);
  return live;
}

/**
 * The tabs worth restoring when the project at `root` opens again: shells and shell
 * profiles with their names and directories. SSH sessions, programs started elsewhere
 * (`docker exec` from Services) and Code With Me terminals only make sense while they run.
 */
export function terminalLayoutOf(list: TerminalTab[], active: string | null, root: string, profiles: ShellProfile[]): TerminalLayout {
  const kept = list.filter((t) => !t.ssh && !t.source && (!t.command || t.profile));
  return {
    tabs: kept.map((t) => {
      const profile = t.profile ? profiles.find((p) => p.label === t.profile)?.command ?? t.command : undefined;
      return {
        id: t.id,
        title: t.title,
        ...(t.renamed ? { renamed: true } : {}),
        ...(startDir(t, root) ? { cwd: startDir(t, root) } : {}),
        ...(t.profile && profile ? { profile: { label: t.profile, program: profile.program, args: profile.args, ...(profile.env?.length ? { env: profile.env } : {}) } } : {}),
      };
    }),
    ...(active && kept.some((t) => t.id === active) ? { active } : {}),
  };
}

/** Tabs for a saved layout; a profile that still exists starts as it is configured now. */
export function tabsFromLayout(layout: TerminalLayout, profiles: ShellProfile[]): TerminalTab[] {
  return layout.tabs.map((s) => {
    const current = s.profile && profiles.find((p) => p.label === s.profile!.label)?.command;
    const command = s.profile ? (current ?? { program: s.profile.program, args: s.profile.args, env: s.profile.env }) : undefined;
    return {
      id: s.id,
      title: s.title,
      ...(s.renamed ? { renamed: true } : {}),
      ...(s.cwd ? { cwd: s.cwd } : {}),
      ...(command ? { command: { program: command.program, args: command.args, env: command.env, local: command.local }, profile: s.profile!.label } : {}),
    };
  });
}
