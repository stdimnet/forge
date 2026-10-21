/**
 * OSC (operating system command) sequences Forge's terminal understands beyond what xterm.js
 * handles itself: OSC 9;4 progress, OSC 9 / OSC 777 notifications, OSC 7 (and ConEmu 9;9)
 * working directories and OSC 133 command boundaries, plus OSC 8 link targets.
 *
 * The payloads are parsed in Rust (`forge_output::osc`). xterm.js runs OSC handlers inside its
 * parser and needs the answer before the handler returns, so the parsers run synchronously
 * through WebAssembly in the desktop app and in browser mode alike (like the keymap resolver).
 * Until the module is loaded, sequences are ignored.
 */
import { forgeWasm, type ForgeWasm } from "@/wasm";

export type ProgressState = "normal" | "error" | "indeterminate" | "paused";

export interface Progress {
  state: ProgressState;
  /** 0–100. Indeterminate progress keeps the last known value. */
  percent: number;
}

/** What a sequence asks the terminal to do (`forge_output::osc::OscAction`). */
export type OscAction =
  | { kind: "progress"; progress: Progress | null }
  | { kind: "notify"; title?: string; body: string }
  | { kind: "cwd"; path: string }
  | { kind: "commandBoundary" }
  | { kind: "ignored" };

/** Where an OSC 8 hyperlink leads; a file `path` is workspace-relative. */
export type LinkTarget = { kind: "url"; url: string } | { kind: "file"; path: string; line?: number } | null;

let wasm: ForgeWasm | null = null;

/** Resolves once the parsers are loaded; loading starts when the terminal is imported. */
export const oscReady: Promise<boolean> = forgeWasm().then(
  (w) => {
    wasm = w;
    return true;
  },
  (e) => {
    console.warn("terminal: sequence parsers not loaded", e);
    return false;
  },
);

function call<T>(fn: (json: string) => string, input: unknown, fallback: T): T {
  const out = JSON.parse(fn(JSON.stringify(input))) as T | { error: string };
  if (out && typeof out === "object" && "error" in out && Object.keys(out).length === 1) {
    console.warn("terminal:", (out as { error: string }).error);
    return fallback;
  }
  return out as T;
}

/**
 * Parse the payload of OSC `code` (7, 9, 133 or 777), i.e. what follows `ESC ] <code> ;`.
 * `previous` is the progress the tab shows now: error and paused reports without a
 * percentage keep its value.
 */
export function parseOsc(code: number, data: string, previous?: Progress | null): OscAction {
  const ignored: OscAction = { kind: "ignored" };
  return wasm ? call(wasm.terminal_parse_osc, { code, data, previous: previous ?? null }, ignored) : ignored;
}

/**
 * What an OSC 8 hyperlink points to. Only web and mail links open in the browser and
 * `file://` links into the project (`root`) open in the editor; everything else (e.g.
 * `javascript:`) is refused with `null`.
 */
export function linkTarget(uri: string, root?: string | null): LinkTarget {
  return wasm ? call<LinkTarget>(wasm.terminal_link_target, { uri, root: root ?? null }, null) : null;
}

/** Short label for a progress value: "42%", "Working…", "Error 42%", "Paused 42%". */
export function progressLabel(p: Progress): string {
  switch (p.state) {
    case "indeterminate":
      return "Working…";
    case "error":
      return p.percent > 0 ? `Error ${p.percent}%` : "Error";
    case "paused":
      return p.percent > 0 ? `Paused ${p.percent}%` : "Paused";
    default:
      return `${p.percent}%`;
  }
}

/** Keys a modifier combination the terminal translates itself (so TUIs see them). */
export interface KeyLike {
  key: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}

/**
 * Sequences xterm.js would not send but TUI apps such as Claude Code expect. xterm.js sends
 * a plain CR for Shift+Enter and Ctrl+Enter, which submits; Claude Code's `/terminal-setup`
 * makes VS Code send ESC CR for Shift+Enter (the same as Alt+Enter), which inserts a newline.
 * Ctrl+Enter sends LF (Ctrl+J), as Windows Terminal does; shells treat it like Enter and
 * Claude Code inserts a newline.
 */
export function terminalKeySequence(e: KeyLike): string | undefined {
  if (e.key !== "Enter" || e.metaKey) return undefined;
  if (e.shiftKey && !e.ctrlKey && !e.altKey) return "\x1b\r";
  if (e.ctrlKey && !e.shiftKey && !e.altKey) return "\n";
  return undefined;
}

/** Links open on Ctrl+Click (Cmd+Click on macOS), so a plain click still selects text. */
export const isLinkActivation = (e: { ctrlKey: boolean; metaKey: boolean }) => e.ctrlKey || e.metaKey;
