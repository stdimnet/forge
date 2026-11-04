/**
 * xterm.js instances of the terminal tabs, connected to backend PTY sessions. Instances
 * live outside Solid so switching tabs or hiding the tool window keeps scrollback.
 *
 * Besides what xterm.js handles itself (truecolor, underline styles and colors, synchronized
 * output, focus and mouse reporting, bracketed paste, OSC 8 hyperlinks, OSC 0/2 titles), the
 * sessions understand OSC 9;4 progress, OSC 9 / OSC 777 notifications, OSC 7 working
 * directories and OSC 133 command boundaries (see `osc.ts`).
 */
import { createEffect, createRoot, untrack } from "solid-js";
import { Terminal, type IDisposable, type IMarker, type ITheme } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { UnicodeGraphemesAddon } from "@xterm/addon-unicode-graphemes";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { terminalApi, type SshTarget, type TerminalEvent } from "@/ipc/terminal";
import { collabApi } from "@/ipc/collab";
import { getSetting } from "@/state/settings";
import { runApi } from "@/ipc/run";
import { isToolWindowVisible, showToolWindow } from "@/state/layout";
import { workspace } from "@/state/workspace";
import { revealLocation } from "@/features/lsp/navigation";
import { showBalloon } from "@/keymap";
import { activeTab, closeTab, displayTitle, setActiveTab, tabs, updateTab, type TerminalProgram, type TerminalSource } from "./store";
import { isLinkActivation, linkTarget, parseOsc, terminalKeySequence, type Progress } from "./osc";
import { terminalOptions, terminalRenderer, type TerminalRenderer } from "./options";
import { t as tr } from "@/i18n";
import { readClipboard, writeClipboard } from "@/features/clipboard/system";

interface Session {
  term: Terminal;
  fit: FitAddon;
  host: HTMLElement;
  opened: boolean;
  exited: boolean;
  webgl?: WebglAddon;
  /** Renderer in use; "dom" after a failed or lost WebGL context. */
  renderer: Exclude<TerminalRenderer, "auto">;
  unlisten?: () => void;
  lastSize?: string;
  disposables: IDisposable[];
  /** Another machine's size, kept instead of fitting (a Code With Me host's terminal). */
  fixed?: { cols: number; rows: number };
  /** Keys are not sent (a guest that may only watch the host's terminal). */
  readOnly?: boolean;
  /** A Code With Me host's terminal: its tab stays when it ends. */
  shared?: boolean;
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

async function openFileLink(rel: string, line?: number, column?: number): Promise<void> {
  const ok = await revealLocation(rel, { line: Math.max(0, (line ?? 1) - 1), character: Math.max(0, (column ?? 1) - 1) });
  if (!ok) showBalloon(tr("tools.terminal.cannotOpen", { rel }));
}

function openUrl(uri: string): void {
  window.open(uri, "_blank", "noopener");
}

/** Open an OSC 8 hyperlink or a detected URL. */
function openLink(uri: string): void {
  const target = linkTarget(uri, workspace()?.root);
  if (target?.kind === "url") openUrl(target.url);
  else if (target?.kind === "file") void openFileLink(target.path, target.line);
  else showBalloon(tr("tools.terminal.cannotOpenLink"), uri);
}

const linkHint = (s: Session) => s.host.querySelector<HTMLElement>(".term-link-hint");

/** Show the link target at the bottom of the terminal, like a browser's status bubble. */
function showLinkHint(s: Session, uri: string): void {
  let el = linkHint(s);
  if (!el) {
    el = document.createElement("div");
    // `xterm-hover` keeps xterm from treating the hint as terminal content.
    el.className = "term-link-hint xterm-hover";
    s.host.appendChild(el);
  }
  const mac = typeof navigator !== "undefined" && /Mac/.test(navigator.platform);
  el.textContent = `${uri}  ·  ${mac ? "⌘" : "Ctrl"}+Click to open`;
}

const hideLinkHint = (s: Session) => linkHint(s)?.remove();

export async function copySelection(id: string): Promise<void> {
  const sel = sessions.get(id)?.term.getSelection();
  if (sel) await writeClipboard(sel);
}

/** Paste the clipboard; `term.paste` wraps it in bracketed paste markers when the shell asks for them. */
export async function paste(id: string): Promise<void> {
  const text = await readClipboard();
  const s = sessions.get(id);
  if (s && text) s.term.paste(text);
}

export const selectAll = (id: string) => sessions.get(id)?.term.selectAll();
export const clearBuffer = (id: string) => sessions.get(id)?.term.clear();
export const focusSession = (id: string) => sessions.get(id)?.term.focus();
export const hasSelection = (id: string) => !!sessions.get(id)?.term.hasSelection();
/** The renderer a session draws with ("webgl" or "dom"). */
export const sessionRenderer = (id: string) => sessions.get(id)?.renderer;

/** Whether the tab is on screen: the Terminal tool window is open and the tab selected. */
const isVisible = (id: string) => isToolWindowVisible("terminal") && activeTab() === id;

/** Open the Terminal tool window on a tab (status bar item, notification "Show"). */
export function revealTerminalTab(id: string): void {
  showToolWindow("terminal");
  setActiveTab(id);
  requestAnimationFrame(() => {
    fitSession(id);
    focusSession(id);
  });
}

/** Fit the terminal to its host and tell the shell about the new size. */
export function fitSession(id: string): void {
  const s = sessions.get(id);
  const el = s?.term.element;
  if (!s || !el || el.offsetParent === null || el.clientWidth === 0) return;
  // The host's size wins: a guest renders the host's terminal at its columns and rows.
  if (s.fixed) {
    if (s.term.cols !== s.fixed.cols || s.term.rows !== s.fixed.rows) s.term.resize(s.fixed.cols, s.fixed.rows);
    return;
  }
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

/** Switch a session to `wanted`, falling back to the DOM renderer when WebGL fails. */
function applyRenderer(id: string, s: Session, wanted: TerminalRenderer): void {
  const useWebgl = wanted !== "dom";
  if (useWebgl && !s.webgl) {
    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => {
        // GPU reset or too many contexts: keep the session usable with the DOM renderer.
        webgl.dispose();
        if (s.webgl === webgl) s.webgl = undefined;
        setRenderer(s, "dom");
      });
      s.term.loadAddon(webgl);
      s.webgl = webgl;
      setRenderer(s, "webgl");
    } catch (err) {
      if (wanted === "webgl") console.warn(`terminal ${id}: WebGL renderer unavailable, using DOM`, err);
      setRenderer(s, "dom");
    }
  } else if (!useWebgl && s.webgl) {
    s.webgl.dispose();
    s.webgl = undefined;
    setRenderer(s, "dom");
  }
}

function setRenderer(s: Session, r: Session["renderer"]): void {
  s.renderer = r;
  s.host.dataset.renderer = r;
}

function loadUnicode(term: Terminal): void {
  // Grapheme clusters and Unicode 15 widths: emoji, ZWJ sequences and East Asian wide
  // characters take two cells, as in kitty and the `string-width` package TUIs use.
  try {
    term.loadAddon(new UnicodeGraphemesAddon());
    return;
  } catch {
    /* fall back to Unicode 11 widths */
  }
  try {
    term.loadAddon(new Unicode11Addon());
    term.unicode.activeVersion = "11";
  } catch {
    /* keep xterm's Unicode 6 widths */
  }
}

let beepCtx: AudioContext | undefined;
function beep(): void {
  try {
    beepCtx ??= new AudioContext();
    const osc = beepCtx.createOscillator();
    const gain = beepCtx.createGain();
    osc.frequency.value = 880;
    gain.gain.setValueAtTime(0.06, beepCtx.currentTime);
    gain.gain.exponentialRampToValueAtTime(0.0001, beepCtx.currentTime + 0.12);
    osc.connect(gain).connect(beepCtx.destination);
    osc.start();
    osc.stop(beepCtx.currentTime + 0.12);
  } catch {
    /* no audio */
  }
}

function ringBell(id: string, s: Session): void {
  if (getSetting<boolean>("terminal.audibleBell", false)) beep();
  if (isVisible(id)) {
    // Visual bell: a short flash of the terminal.
    s.host.classList.remove("term-bell");
    void s.host.offsetWidth;
    s.host.classList.add("term-bell");
    setTimeout(() => s.host.classList.remove("term-bell"), 200);
  } else {
    updateTab(id, { bell: true });
  }
}

const currentTab = (id: string) => tabs().find((t) => t.id === id);

function setProgress(id: string, progress: Progress | null | undefined): void {
  const cur = currentTab(id)?.progress;
  const next = progress ?? undefined;
  if (cur?.state === next?.state && cur?.percent === next?.percent) return;
  updateTab(id, { progress: next });
}

function notify(id: string, title: string | undefined, body: string): void {
  // Like kitty, stay quiet when the user is looking at the tab.
  if (isVisible(id) && document.hasFocus()) return;
  const tab = currentTab(id);
  const name = tab ? displayTitle(tab) : "Terminal";
  showBalloon(title ? `${name}: ${title}` : name, body, 8000, [{ label: "Show", run: () => revealTerminalTab(id) }]);
}

/** Act on an OSC sequence (parsed in Rust, see `osc.ts`). */
function handleOsc(id: string, code: number, data: string): void {
  const r = parseOsc(code, data, currentTab(id)?.progress);
  if (r.kind === "progress") setProgress(id, r.progress);
  else if (r.kind === "notify") notify(id, r.title, r.body);
  else if (r.kind === "cwd") {
    if (currentTab(id)?.liveCwd !== r.path) updateTab(id, { liveCwd: r.path });
  }
  // A new prompt means the previous program is gone; drop progress it left behind.
  else if (r.kind === "commandBoundary") setProgress(id, null);
}

/** Wire OSC handlers, title, bell and keys of a new session. */
/**
 * Shell integration (Settings | Tools | Terminal; the scripts come from Rust,
 * `forge_process::integration`): finished commands (OSC 133 `C` … `D;<code>`) get a green or
 * red mark in the overview ruler.
 */
function commandMarks(term: Terminal): IDisposable {
  let command: IMarker | undefined;
  return term.parser.registerOscHandler(133, (data) => {
    const [kind, code] = data.split(";");
    if (kind === "C") command = term.registerMarker(0);
    else if (kind === "D" && command) {
      const failed = code !== undefined && code !== "0";
      term.registerDecoration({
        marker: command,
        overviewRulerOptions: { color: cssVar(failed ? "--danger" : "--success", failed ? "#e5534b" : "#57ab5a"), position: "left" },
      });
      command = undefined;
    }
    return false;
  });
}

function registerHandlers(id: string, s: Session): void {
  const { term } = s;
  const parser = term.parser;
  s.disposables.push(
    // 9: progress, notifications, ConEmu cwd; 777: notifications; 7: working directory.
    ...[9, 777, 7].map((code) =>
      parser.registerOscHandler(code, (data) => {
        handleOsc(id, code, data);
        return true;
      }),
    ),
    // OSC 133 marks stay visible to other handlers.
    parser.registerOscHandler(133, (data) => {
      handleOsc(id, 133, data);
      return false;
    }),
    commandMarks(term),
    term.onTitleChange((title) => {
      if (currentTab(id)?.appTitle !== title) updateTab(id, { appTitle: title });
    }),
    term.onBell(() => ringBell(id, s)),
  );

  // Terminal-local keys: clipboard (PHPStorm: Ctrl+Shift+C / Ctrl+Shift+V, Ctrl+C with a
  // selection copies) and the Enter variants TUIs need. This is xterm's own key handling,
  // not IDE shortcuts; those go through the keymap.
  term.attachCustomKeyEventHandler((e) => {
    const seq = terminalKeySequence(e);
    if (seq !== undefined) {
      if (e.type === "keydown") term.input(seq, true);
      e.preventDefault();
      return false;
    }
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
}

/** A shared terminal's size or access changed on the host (Code With Me guest). */
export function updateSharedSession(id: string, cols: number, rows: number, readOnly: boolean): void {
  const s = sessions.get(id);
  if (!s) return;
  s.fixed = { cols, rows };
  s.readOnly = readOnly;
  s.term.options.disableStdin = readOnly;
  fitSession(id);
}

/** Create the xterm instance for a tab inside `host` and start its shell (or attach to `source`). */
export function attachSession(id: string, host: HTMLElement, cwd?: string, command?: TerminalProgram, ssh?: SshTarget, source?: TerminalSource): void {
  const existing = sessions.get(id);
  if (existing) {
    if (existing.term.element && existing.term.element.parentElement !== host) host.appendChild(existing.term.element);
    existing.host = host;
    setRenderer(existing, existing.renderer);
    return;
  }
  let session!: Session;
  const term = new Terminal({
    ...terminalOptions(getSetting),
    allowProposedApi: true,
    theme: terminalTheme(),
    rightClickSelectsWord: false,
    // Box drawing and block elements drawn by the renderer line up without gaps.
    customGlyphs: true,
    rescaleOverlappingGlyphs: true,
    // Command marks of shell integration (green: succeeded, red: failed).
    overviewRuler: { width: 8 },
    linkHandler: {
      allowNonHttpProtocols: true,
      activate: (e, uri) => {
        if (isLinkActivation(e)) openLink(uri);
      },
      hover: (_e, uri) => showLinkHint(session, uri),
      leave: () => hideLinkHint(session),
    },
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  loadUnicode(term);
  session = { term, fit, host, opened: false, exited: false, renderer: "dom", disposables: [] };
  term.loadAddon(
    new WebLinksAddon(
      (e, uri) => {
        if (isLinkActivation(e)) openUrl(uri);
      },
      { hover: (_e, uri) => showLinkHint(session, uri), leave: () => hideLinkHint(session) },
    ),
  );
  term.registerLinkProvider({
    provideLinks(y, callback) {
      const line = term.buffer.active.getLine(y - 1)?.translateToString(true) ?? "";
      // File references are found by the backend (`forge-output`); only lines that could hold one are sent.
      if (!/[\\/.]/.test(line)) return callback(undefined);
      runApi
        .fileLinks(line)
        .catch(() => [])
        .then((links) =>
          callback(
            links.length
              ? links.map((l) => ({
                  range: { start: { x: l.start + 1, y }, end: { x: l.end, y } },
                  text: line.slice(l.start, l.end),
                  decorations: { underline: true, pointerCursor: true },
                  activate: () => void openFileLink(l.target ?? l.path, l.line, l.column),
                }))
              : undefined,
          ),
        );
    },
  });
  registerHandlers(id, session);
  term.onSelectionChange(() => {
    if (getSetting<boolean>("terminal.copyOnSelection", false) && term.hasSelection()) void copySelection(id);
  });
  term.open(host);
  sessions.set(id, session);
  applyRenderer(id, session, terminalRenderer(getSetting));
  try {
    fit.fit();
  } catch {
    /* host not laid out yet; the resize observer fits later */
  }
  term.onData((data) => {
    if (session.opened && !session.exited && !session.readOnly) void terminalApi.write(id, data).catch(() => undefined);
  });
  if (source) void attachExisting(id, session, source);
  else void start(id, session, cwd, command, ssh);
}

let sshHostKeys: (target: SshTarget) => Promise<boolean> = async () => false;
/** Asks about an unknown or changed host key of an SSH session (the SSH feature sets it). */
export function setSshHostKeyResolver(resolve: (target: SshTarget) => Promise<boolean>): void {
  sshHostKeys = resolve;
}

/** Output and exit of a session's backend. */
function handleEvent(id: string, s: Session, e: TerminalEvent): void {
  if (e.type === "output") s.term.write(e.data);
  else {
    s.exited = true;
    setProgress(id, null);
    if (s.shared) {
      // The host's terminal ended: the guest's tab keeps its output, like a terminated session.
      s.term.options.disableStdin = true;
      s.term.write(`\r\n\x1b[2m[Process completed${e.code ? ` with exit code ${e.code}` : ""}]\x1b[0m\r\n`);
    } else if (e.code === 0 || e.code === null) {
      disposeSession(id);
      closeTab(id);
    } else s.term.write(`\r\n\x1b[2m[Process completed with exit code ${e.code}]\x1b[0m\r\n`);
  }
}

/**
 * Show a session that already runs: what it showed so far, then its live output. Output
 * that arrives while the earlier output loads is written after it.
 */
async function attachExisting(id: string, s: Session, source: TerminalSource): Promise<void> {
  let queued: TerminalEvent[] | null = [];
  s.unlisten = await terminalApi.onEvent(id, (e) => (queued ? queued.push(e) : handleEvent(id, s, e)));
  try {
    if (source.kind === "collab") {
      s.shared = true;
      if (source.ended) {
        s.exited = true;
        s.term.options.disableStdin = true;
        s.term.write(`\x1b[2m[${source.ended}]\x1b[0m\r\n`);
        s.opened = true;
        return;
      }
      const r = await collabApi.terminalAttach(source.terminal, id);
      updateSharedSession(id, r.cols, r.rows, source.readOnly);
      if (r.scrollback) s.term.write(r.scrollback);
      if (!r.running) {
        s.exited = true;
        s.term.write(`\r\n\x1b[2m[Process completed]\x1b[0m\r\n`);
      }
    } else {
      const r = await terminalApi.attach(id);
      if (r.scrollback) s.term.write(r.scrollback);
    }
    s.opened = true;
    s.lastSize = `${s.term.cols}x${s.term.rows}`;
  } catch (err) {
    s.exited = true;
    s.term.write(`\x1b[31mCannot show the terminal: ${String((err as Error)?.message ?? err)}\x1b[0m\r\n`);
  }
  const pending = queued;
  queued = null;
  for (const e of pending) handleEvent(id, s, e);
}

async function start(id: string, s: Session, cwd?: string, command?: TerminalProgram, ssh?: SshTarget): Promise<void> {
  s.unlisten = await terminalApi.onEvent(id, (e) => handleEvent(id, s, e));
  const open = () =>
    terminalApi.open({
      id,
      cwd,
      cols: s.term.cols,
      rows: s.term.rows,
      ...(command && { shell: command.program, args: command.args, env: Object.fromEntries(command.env ?? []), local: command.local }),
      ...(ssh && { ssh }),
    });
  try {
    if (ssh) s.term.write(`\x1b[2mConnecting…\x1b[0m\r\n`);
    try {
      await open();
    } catch (err) {
      // An unknown host key: ask like Test Connection does, then connect again.
      if (!ssh || !String(err).includes("not trusted yet") || !(await sshHostKeys(ssh))) throw err;
      await open();
    }
    s.opened = true;
    s.lastSize = `${s.term.cols}x${s.term.rows}`;
  } catch (err) {
    s.exited = true;
    const what = ssh ? "Cannot start the SSH session" : "Cannot open the terminal";
    s.term.write(`\x1b[31m${what}: ${String(err)}\x1b[0m\r\n`);
  }
}

/**
 * A Code With Me host's terminal is no longer shown in this tab (the host stopped sharing
 * it): the tab keeps what it showed, takes no more input and says why.
 */
export function endSharedSession(id: string, reason: string): void {
  const s = sessions.get(id);
  if (!s) return;
  s.exited = true;
  setProgress(id, null);
  s.term.options.disableStdin = true;
  s.term.write(`\r\n\x1b[2m[${reason}]\x1b[0m\r\n`);
}

/** Kill the shell and destroy the xterm instance. */
export function disposeSession(id: string): void {
  const s = sessions.get(id);
  if (!s) return;
  sessions.delete(id);
  s.unlisten?.();
  if (s.opened && !s.exited) void terminalApi.close(id).catch(() => undefined);
  for (const d of s.disposables) d.dispose();
  s.webgl?.dispose();
  s.term.dispose();
}

// Settings apply to open sessions right away.
if (typeof window !== "undefined") {
  createRoot(() => {
    createEffect(() => {
      const options = terminalOptions(getSetting);
      const renderer = terminalRenderer(getSetting);
      untrack(() => {
        for (const [id, s] of sessions) {
          Object.assign(s.term.options, options);
          applyRenderer(id, s, renderer);
          if (isVisible(id)) requestAnimationFrame(() => fitSession(id));
        }
      });
    });
  });
}
