/** Terminal sessions on a PTY (Rust: `crates/forge-process`, `commands/terminal.rs`). */
import { invoke, listen } from "./index";

export interface TerminalOpenOptions {
  /** Chosen by the UI so it can subscribe to `terminal://<id>` before the shell starts. */
  id?: string;
  /** Start directory, relative to the project root or absolute; default: project root. */
  cwd?: string;
  /** Shell executable; default: setting `terminal.shellPath` or the system shell. */
  shell?: string;
  /** Arguments for `shell` instead of the shell's defaults. */
  args?: string[];
  env?: Record<string, string>;
  /** Start on this machine (in its home directory) even when the project is remote. */
  local?: boolean;
  /** An SSH session to another host instead of a shell (Tools | Start SSH Session…). */
  ssh?: SshTarget;
  cols: number;
  rows: number;
}

/** What an SSH terminal session connects to. */
export interface SshTarget {
  /** A saved SSH configuration. */
  configId?: string;
  /** An SFTP deployment server (its SSH configuration). */
  serverId?: string;
  /** Password or passphrase typed for this session. */
  secret?: string;
  /** The user confirmed a session to a server of an untrusted project (Safe Mode). */
  confirmed?: boolean;
}

export interface TerminalOpened {
  id: string;
  shell: string;
  cwd: string;
}

/** A shell profile of the "+" menu a restored tab was opened with. */
export interface TerminalProfileState {
  label: string;
  program: string;
  args: string[];
  env?: [string, string][];
}

/** A tab of a project's terminal layout (`forge_process::terminal_layout`). */
export interface TerminalTabState {
  id: string;
  title: string;
  renamed?: boolean;
  /** Relative to the project root, or absolute. */
  cwd?: string;
  profile?: TerminalProfileState;
  /** The tab's saved output (setting `terminal.restoreScrollback`); only in loaded layouts. */
  scrollback?: string;
}

/** The Terminal tool window of a project, restored when it opens again. */
export interface TerminalLayout {
  tabs: TerminalTabState[];
  active?: string;
}

/** A session running a program instead of waiting at its prompt. */
export interface RunningCommand {
  id: string;
  process: string;
}

export type TerminalEvent = { type: "output"; data: string } | { type: "exited"; code: number | null };

export const terminalEvent = (id: string) => `terminal://${id}`;

export const terminalApi = {
  open: (options: TerminalOpenOptions) => invoke<TerminalOpened>("terminal_open", { options }),
  write: (id: string, data: string) => invoke<void>("terminal_write", { id, data }),
  resize: (id: string, cols: number, rows: number) => invoke<void>("terminal_resize", { id, cols, rows }),
  close: (id: string) => invoke<void>("terminal_close", { id }),
  /** A session that already runs (a Code With Me guest opened it on this machine): what it showed so far. */
  attach: (id: string) => invoke<{ scrollback: string; cols: number; rows: number }>("terminal_attach", { id }),
  onEvent: (id: string, cb: (e: TerminalEvent) => void) => listen<TerminalEvent>(terminalEvent(id), cb),
  /** Sessions running a program in the foreground (`sleep 100`); Rust asks the PTY. */
  running: () => invoke<RunningCommand[]>("terminal_running"),
  /** The terminal layout of the project at `root`, with the saved output when that option is on. */
  loadLayout: (root: string) => invoke<TerminalLayout | null>("terminal_layout_load", { root }),
  saveLayout: (root: string, layout: TerminalLayout) => invoke<void>("terminal_layout_save", { root, layout }),
};
