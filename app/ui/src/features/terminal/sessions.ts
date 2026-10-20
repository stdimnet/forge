/**
 * xterm.js instances of the terminal tabs, connected to backend PTY sessions. Instances
 * live outside Solid so switching tabs or hiding the tool window keeps scrollback.
 */
import { Terminal, type ITheme } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { terminalApi } from "@/ipc/terminal";
import { getSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { revealLocation } from "@/features/lsp/navigation";
import { findFileLinks, toWorkspacePath } from "@/features/run/fileLinks";
import { showBalloon } from "@/keymap";
import { closeTab } from "./store";

interface Session {
  term: Terminal;
  fit: FitAddon;
  opened: boolean;
  exited: boolean;
  unlisten?: () => void;
  lastSize?: string;
}

const sessions = new Map<string, Session>();

const cssVar = (name: string, fallback: string) =>
  (typeof document !== "undefined" && getComputedStyle(document.documentElement).getPropertyValue(name).trim()) || fallback;

/** Terminal colors from the theme's CSS variables. */
export function terminalTheme(): ITheme {
  const v = (n: string, f = "#888") => cssVar(n, f);
  return {
    background: v("--bg-panel", "#18181c"),
    foreground: v("--fg", "#d9d9df"),
    cursor: v("--accent", "#ff7a1a"),
    cursorAccent: v("--bg-panel", "#18181c"),
    selectionBackground: v("--bg-selection", "#3a2a1e"),
    black: v("--ansi-black"),
    red: v("--ansi-red"),
    green: v("--ansi-green"),
    yellow: v("--ansi-yellow"),
    blue: v("--ansi-blue"),
    magenta: v("--ansi-magenta"),
    cyan: v("--ansi-cyan"),
    white: v("--ansi-white"),
    brightBlack: v("--ansi-bright-black"),
    brightRed: v("--ansi-bright-red"),
    brightGreen: v("--ansi-bright-green"),
    brightYellow: v("--ansi-bright-yellow"),
    brightBlue: v("--ansi-bright-blue"),
    brightMagenta: v("--ansi-bright-magenta"),
    brightCyan: v("--ansi-bright-cyan"),
    brightWhite: v("--ansi-bright-white"),
  };
}

export function applyThemeToAll(): void {
  const theme = terminalTheme();
  for (const s of sessions.values()) s.term.options.theme = theme;
}

async function openFileLink(path: string, line?: number, column?: number): Promise<void> {
  const rel = toWorkspacePath(path, workspace()?.root);
  if (!rel) return;
  const ok = await revealLocation(rel, { line: Math.max(0, (line ?? 1) - 1), character: Math.max(0, (column ?? 1) - 1) });
  if (!ok) showBalloon(`Cannot open ${rel}`);
}

function openUrl(uri: string): void {
  window.open(uri, "_blank", "noopener");
}

export async function copySelection(id: string): Promise<void> {
  const sel = sessions.get(id)?.term.getSelection();
  if (sel) await navigator.clipboard?.writeText(sel).catch(() => undefined);
}

export async function paste(id: string): Promise<void> {
  const s = sessions.get(id);
  const text = await navigator.clipboard?.readText().catch(() => "");
  if (s && text) s.term.paste(text);
}

export const selectAll = (id: string) => sessions.get(id)?.term.selectAll();
export const clearBuffer = (id: string) => sessions.get(id)?.term.clear();
export const focusSession = (id: string) => sessions.get(id)?.term.focus();
export const hasSelection = (id: string) => !!sessions.get(id)?.term.hasSelection();

/** Fit the terminal to its host and tell the shell about the new size. */
export function fitSession(id: string): void {
  const s = sessions.get(id);
  const el = s?.term.element;
  if (!s || !el || el.offsetParent === null || el.clientWidth === 0) return;
  try {
    s.fit.fit();
  } catch {
    return;
  }
  const size = `${s.term.cols}x${s.term.rows}`;
  if (s.opened && !s.exited && size !== s.lastSize) {
    s.lastSize = size;
    void terminalApi.resize(id, s.term.cols, s.term.rows).catch(() => undefined);
  }
}

/** Create the xterm instance for a tab inside `host` and start its shell. */
export function attachSession(id: string, host: HTMLElement, cwd?: string): void {
  const existing = sessions.get(id);
  if (existing) {
    if (existing.term.element && existing.term.element.parentElement !== host) host.appendChild(existing.term.element);
    return;
  }
  const term = new Terminal({
    fontFamily: cssVar("--font-mono", "monospace"),
    fontSize: 13,
    lineHeight: 1.2,
    cursorBlink: true,
    cursorStyle: "block",
    scrollback: 10_000,
    allowProposedApi: true,
    theme: terminalTheme(),
    macOptionIsMeta: true,
    rightClickSelectsWord: false,
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.loadAddon(new WebLinksAddon((_e, uri) => openUrl(uri)));
  term.registerLinkProvider({
    provideLinks(y, callback) {
      const line = term.buffer.active.getLine(y - 1)?.translateToString(true) ?? "";
      const links = findFileLinks(line);
      callback(
        links.length
          ? links.map((l) => ({
              range: { start: { x: l.start + 1, y }, end: { x: l.end, y } },
              text: line.slice(l.start, l.end),
              decorations: { underline: true, pointerCursor: true },
              activate: () => void openFileLink(l.path, l.line, l.column),
            }))
          : undefined,
      );
    },
  });
  // Terminal-local clipboard keys (PHPStorm: Ctrl+Shift+C / Ctrl+Shift+V, Ctrl+C with a
  // selection copies). These are terminal input handling, not IDE shortcuts.
  term.attachCustomKeyEventHandler((e) => {
    if (e.type !== "keydown") return true;
    const k = e.key.toLowerCase();
    if ((e.ctrlKey && e.shiftKey && k === "c") || (e.metaKey && k === "c") || (e.ctrlKey && !e.shiftKey && k === "c" && term.hasSelection())) {
      void copySelection(id);
      if (!e.metaKey) term.clearSelection();
      e.preventDefault();
      return false;
    }
    if ((e.ctrlKey && e.shiftKey && k === "v") || (e.metaKey && k === "v")) {
      void paste(id);
      e.preventDefault();
      return false;
    }
    return true;
  });
  term.onSelectionChange(() => {
    if (getSetting<boolean>("terminal.copyOnSelection", false) && term.hasSelection()) void copySelection(id);
  });
  term.open(host);
  const session: Session = { term, fit, opened: false, exited: false };
  sessions.set(id, session);
  try {
    fit.fit();
  } catch {
    /* host not laid out yet; the resize observer fits later */
  }
  term.onData((data) => {
    if (session.opened && !session.exited) void terminalApi.write(id, data).catch(() => undefined);
  });
  void start(id, session, cwd);
}

async function start(id: string, s: Session, cwd?: string): Promise<void> {
  s.unlisten = await terminalApi.onEvent(id, (e) => {
    if (e.type === "output") s.term.write(e.data);
    else {
      s.exited = true;
      if (e.code === 0 || e.code === null) {
        disposeSession(id);
        closeTab(id);
      } else s.term.write(`\r\n\x1b[2m[Process completed with exit code ${e.code}]\x1b[0m\r\n`);
    }
  });
  try {
    await terminalApi.open({ id, cwd, cols: s.term.cols, rows: s.term.rows });
    s.opened = true;
    s.lastSize = `${s.term.cols}x${s.term.rows}`;
  } catch (err) {
    s.exited = true;
    s.term.write(`\x1b[31mCannot open the terminal: ${String(err)}\x1b[0m\r\n`);
  }
}

/** Kill the shell and destroy the xterm instance. */
export function disposeSession(id: string): void {
  const s = sessions.get(id);
  if (!s) return;
  sessions.delete(id);
  s.unlisten?.();
  if (s.opened && !s.exited) void terminalApi.close(id).catch(() => undefined);
  s.term.dispose();
}
