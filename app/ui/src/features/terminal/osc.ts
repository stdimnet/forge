/**
 * Parsers for the OSC (operating system command) sequences Forge's terminal understands
 * beyond what xterm.js handles itself. xterm.js calls our handlers with the payload after
 * `ESC ] <ident> ;`, so every parser here takes that payload.
 *
 * - OSC 9;4 progress (ConEmu, Windows Terminal, Ghostty; emitted by Claude Code, winget,
 *   systemd, ...): `ESC ] 9 ; 4 ; <state> ; <percent> ST`.
 * - OSC 9 notifications (iTerm2): `ESC ] 9 ; <message> ST`.
 * - OSC 777 notifications (rxvt, Ghostty, foot): `ESC ] 777 ; notify ; <title> ; <body> ST`.
 * - OSC 7 working directory: `ESC ] 7 ; file://<host>/<path> ST`.
 * - OSC 133 shell integration marks (FinalTerm): `A` prompt start, `D` command finished.
 */

export type ProgressState = "normal" | "error" | "indeterminate" | "paused";

export interface Progress {
  state: ProgressState;
  /** 0–100. Indeterminate progress keeps the last known value. */
  percent: number;
}

export type Osc9 =
  | { kind: "progress"; progress: Progress | null }
  | { kind: "notify"; title?: string; body: string }
  | { kind: "cwd"; path: string }
  | { kind: "ignored" };

const STATES: Record<string, ProgressState | null> = {
  "0": null,
  "1": "normal",
  "2": "error",
  "3": "indeterminate",
  "4": "paused",
};

const clampPercent = (n: number) => Math.max(0, Math.min(100, Math.round(n)));

/**
 * Parse the part after `9;4;`: `<state>[;<percent>]`. Returns `null` for "remove" (state 0),
 * `undefined` for input that is not a valid progress report.
 *
 * Like Windows Terminal, error and paused without a percentage keep `previous`'s value, and
 * normal without a percentage means 0.
 */
export function parseProgress(args: string, previous?: Progress | null): Progress | null | undefined {
  const [rawState = "", rawPercent = ""] = args.split(";");
  const stateKey = rawState.trim();
  if (!(stateKey in STATES)) return undefined;
  const state = STATES[stateKey];
  if (state === null) return null;
  const pct = rawPercent.trim();
  if (pct !== "" && !/^\d+(\.\d+)?$/.test(pct)) return undefined;
  const percent = pct === "" ? (state === "normal" ? 0 : (previous?.percent ?? 0)) : clampPercent(Number(pct));
  return { state, percent };
}

/** Parse an OSC 9 payload: progress (`4;…`), ConEmu cwd (`9;…`) or an iTerm2 notification. */
export function parseOsc9(data: string, previous?: Progress | null): Osc9 {
  const m = /^(\d+)(?:;|$)([\s\S]*)$/.exec(data);
  if (m) {
    // ConEmu uses numeric sub-commands (1 = sleep, 2 = message box, 4 = progress, 9 = cwd …).
    if (m[1] === "4") {
      const progress = parseProgress(m[2]!, previous);
      return progress === undefined ? { kind: "ignored" } : { kind: "progress", progress };
    }
    if (m[1] === "9" && m[2]) return { kind: "cwd", path: unquote(m[2]) };
    return { kind: "ignored" };
  }
  const body = data.trim();
  return body ? { kind: "notify", body } : { kind: "ignored" };
}

/** Parse an OSC 777 payload; only `notify;<title>;<body>` is supported. */
export function parseOsc777(data: string): { title?: string; body: string } | null {
  const [cmd, title = "", ...rest] = data.split(";");
  if (cmd !== "notify") return null;
  const body = rest.join(";").trim();
  if (!title.trim() && !body) return null;
  return body ? { title: title.trim() || undefined, body } : { body: title.trim() };
}

/** Parse an OSC 7 payload (`file://host/path`) into a local path, or `null`. */
export function parseOsc7(data: string): string | null {
  const m = /^file:\/\/([^/]*)(\/.*)$/i.exec(data.trim());
  if (!m) return null;
  try {
    let path = decodeURIComponent(m[2]!);
    // file:///C:/Users/... on Windows
    if (/^\/[A-Za-z]:\//.test(path)) path = path.slice(1);
    return path;
  } catch {
    return null;
  }
}

/** Whether an OSC 133 mark ends the previous command (prompt start or command finished). */
export const isCommandBoundary = (data: string) => data.startsWith("A") || data.startsWith("D");

function unquote(s: string): string {
  const t = s.trim();
  return t.length >= 2 && t.startsWith('"') && t.endsWith('"') ? t.slice(1, -1) : t;
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

export type LinkTarget = { kind: "url"; url: string } | { kind: "file"; path: string; line?: number } | null;

/**
 * What an OSC 8 hyperlink points to. Only web and mail links open in the browser, `file://`
 * links open in the editor; everything else (e.g. `javascript:`) is refused.
 */
export function linkTarget(uri: string): LinkTarget {
  const u = uri.trim();
  if (/^(https?:|mailto:)/i.test(u)) return { kind: "url", url: u };
  if (/^file:\/\//i.test(u)) {
    const [base, fragment] = u.split("#", 2);
    const path = parseOsc7(base!);
    if (!path) return null;
    const line = fragment ? /^L?(\d+)/i.exec(fragment)?.[1] : undefined;
    return { kind: "file", path, line: line ? Number(line) : undefined };
  }
  return null;
}
