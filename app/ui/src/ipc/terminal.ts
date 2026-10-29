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

export type TerminalEvent = { type: "output"; data: string } | { type: "exited"; code: number | null };

export const terminalEvent = (id: string) => `terminal://${id}`;

export const terminalApi = {
  open: (options: TerminalOpenOptions) => invoke<TerminalOpened>("terminal_open", { options }),
  write: (id: string, data: string) => invoke<void>("terminal_write", { id, data }),
  resize: (id: string, cols: number, rows: number) => invoke<void>("terminal_resize", { id, cols, rows }),
  close: (id: string) => invoke<void>("terminal_close", { id }),
  onEvent: (id: string, cb: (e: TerminalEvent) => void) => listen<TerminalEvent>(terminalEvent(id), cb),
};
