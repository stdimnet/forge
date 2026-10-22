import { beforeAll, describe, expect, it, vi } from "vitest";
import { Terminal } from "@xterm/xterm";
import { UnicodeGraphemesAddon } from "@xterm/addon-unicode-graphemes";
import { isLinkActivation, linkTarget, oscReady, parseOsc, progressLabel, terminalKeySequence, type Progress } from "./osc";
import { overallProgress, progressTab, tabLabel, type TerminalTab } from "./store";
import { FALLBACK_FONTS, terminalFontFamily, terminalOptions, terminalRenderer } from "./options";
import { cellWidth, replayExit, replayFrames } from "@/mock/terminalReplays";

vi.hoisted(() => {
  // jsdom has no canvas; xterm.js only needs it for rendering, which these tests skip.
  HTMLCanvasElement.prototype.getContext = (() => null) as typeof HTMLCanvasElement.prototype.getContext;
});

// The parsers are Rust (`forge_output::osc`, tested there); these tests cover the bridge.
beforeAll(async () => {
  expect(await oscReady).toBe(true);
});

describe("OSC 9;4 progress", () => {
  it("parses every state", () => {
    expect(parseOsc(9, "4;1;42")).toEqual({ kind: "progress", progress: { state: "normal", percent: 42 } });
    expect(parseOsc(9, "4;2;60")).toEqual({ kind: "progress", progress: { state: "error", percent: 60 } });
    expect(parseOsc(9, "4;3")).toEqual({ kind: "progress", progress: { state: "indeterminate", percent: 0 } });
    expect(parseOsc(9, "4;4;75")).toEqual({ kind: "progress", progress: { state: "paused", percent: 75 } });
    expect(parseOsc(9, "4;0")).toEqual({ kind: "progress", progress: null });
  });

  it("keeps the previous value for error and paused without a percentage", () => {
    const prev: Progress = { state: "normal", percent: 40 };
    expect(parseOsc(9, "4;2", prev)).toEqual({ kind: "progress", progress: { state: "error", percent: 40 } });
    expect(parseOsc(9, "4;1", prev)).toEqual({ kind: "progress", progress: { state: "normal", percent: 0 } });
  });

  it("ignores invalid input", () => {
    expect(parseOsc(9, "4")).toEqual({ kind: "ignored" });
    expect(parseOsc(9, "4;1;-5")).toEqual({ kind: "ignored" });
  });

  it("labels progress for the status bar", () => {
    expect(progressLabel({ state: "normal", percent: 42 })).toBe("42%");
    expect(progressLabel({ state: "indeterminate", percent: 0 })).toBe("Working…");
    expect(progressLabel({ state: "error", percent: 60 })).toBe("Error 60%");
    expect(progressLabel({ state: "paused", percent: 0 })).toBe("Paused");
  });
});

describe("other OSC sequences", () => {
  it("parses notifications, working directories and command boundaries", () => {
    expect(parseOsc(9, "Claude is waiting for your input")).toEqual({ kind: "notify", body: "Claude is waiting for your input" });
    expect(parseOsc(777, "notify;Claude Code;Task finished")).toEqual({ kind: "notify", title: "Claude Code", body: "Task finished" });
    expect(parseOsc(7, "file:///home/dev/My%20Project")).toEqual({ kind: "cwd", path: "/home/dev/My Project" });
    expect(parseOsc(9, '9;"C:\\Users\\dev"')).toEqual({ kind: "cwd", path: "C:\\Users\\dev" });
    expect(parseOsc(133, "A")).toEqual({ kind: "commandBoundary" });
    expect(parseOsc(133, "B")).toEqual({ kind: "ignored" });
  });

  it("opens only safe OSC 8 link targets, on Ctrl/Cmd+Click", () => {
    const root = "/home/dev/shop";
    expect(linkTarget("https://laravel.com/docs", root)).toEqual({ kind: "url", url: "https://laravel.com/docs" });
    expect(linkTarget("mailto:dev@example.com", root)?.kind).toBe("url");
    expect(linkTarget("file://forge/home/dev/shop/app/Models/Order.php#L30", root)).toEqual({
      kind: "file",
      path: "app/Models/Order.php",
      line: 30,
    });
    expect(linkTarget("file:///etc/passwd", root)).toBeNull();
    expect(linkTarget("javascript:alert(1)", root)).toBeNull();
    expect(isLinkActivation({ ctrlKey: true, metaKey: false })).toBe(true);
    expect(isLinkActivation({ ctrlKey: false, metaKey: true })).toBe(true);
    expect(isLinkActivation({ ctrlKey: false, metaKey: false })).toBe(false);
  });
});

describe("terminal keys", () => {
  const key = (k: string, mods: Partial<Record<"shiftKey" | "ctrlKey" | "altKey" | "metaKey", boolean>> = {}) => ({
    key: k,
    shiftKey: false,
    ctrlKey: false,
    altKey: false,
    metaKey: false,
    ...mods,
  });

  it("sends ESC CR for Shift+Enter like Claude Code's /terminal-setup", () => {
    expect(terminalKeySequence(key("Enter", { shiftKey: true }))).toBe("\x1b\r");
  });

  it("sends LF for Ctrl+Enter", () => {
    expect(terminalKeySequence(key("Enter", { ctrlKey: true }))).toBe("\n");
  });

  it("leaves everything else to xterm.js", () => {
    expect(terminalKeySequence(key("Enter"))).toBeUndefined();
    expect(terminalKeySequence(key("Enter", { altKey: true }))).toBeUndefined();
    expect(terminalKeySequence(key("Enter", { ctrlKey: true, shiftKey: true }))).toBeUndefined();
    expect(terminalKeySequence(key("Enter", { metaKey: true, shiftKey: true }))).toBeUndefined();
    expect(terminalKeySequence(key("Escape"))).toBeUndefined();
    expect(terminalKeySequence(key("c", { ctrlKey: true }))).toBeUndefined();
  });
});

describe("tab titles and status", () => {
  const tab = (id: string, extra: Partial<TerminalTab> = {}): TerminalTab => ({ id, title: id === "a" ? "Local" : "Local (2)", ...extra });

  it("shows the application title unless renamed or turned off", () => {
    expect(tabLabel(tab("a"))).toBe("Local");
    expect(tabLabel(tab("a", { appTitle: "✳ Claude Code" }))).toBe("✳ Claude Code");
    expect(tabLabel(tab("a", { appTitle: "  " }))).toBe("Local");
    expect(tabLabel(tab("a", { appTitle: "vim", renamed: true, title: "Editor" }))).toBe("Editor");
    expect(tabLabel(tab("a", { appTitle: "vim" }), false)).toBe("Local");
  });

  it("picks the active tab's progress for the status bar, else the first", () => {
    const a = tab("a", { progress: { state: "normal", percent: 20 } });
    const b = tab("b", { progress: { state: "indeterminate", percent: 0 } });
    expect(progressTab([a, b], "b")?.id).toBe("b");
    expect(progressTab([a, b], "x")?.id).toBe("a");
    expect(progressTab([tab("a"), b], "a")?.id).toBe("b");
    expect(progressTab([tab("a")], "a")).toBeUndefined();
  });

  it("combines progress for the stripe icon", () => {
    const p = (state: Progress["state"], percent: number) => tab("x", { progress: { state, percent } });
    expect(overallProgress([tab("a")])).toBeUndefined();
    expect(overallProgress([p("normal", 80), p("normal", 30)])).toEqual({ state: "normal", percent: 30 });
    expect(overallProgress([p("normal", 80), p("error", 10)])?.state).toBe("error");
    expect(overallProgress([p("paused", 50), p("normal", 10)])?.state).toBe("paused");
    expect(overallProgress([p("indeterminate", 0), p("normal", 10)])).toEqual({ state: "normal", percent: 10 });
    expect(overallProgress([p("indeterminate", 0)])?.state).toBe("indeterminate");
  });
});

describe("terminal options", () => {
  const settings = (values: Record<string, unknown>) =>
    (<T,>(key: string, fallback: T): T => (key in values ? (values[key] as T) : fallback)) as Parameters<typeof terminalOptions>[0];

  it("builds a font chain with Nerd Font and emoji fallbacks", () => {
    const chain = terminalFontFamily("Iosevka, 'Fira Code'");
    expect(chain.startsWith('"Iosevka", "Fira Code", "JetBrains Mono", "Cascadia Code"')).toBe(true);
    const at = (f: string) => chain.indexOf(`"${f}"`);
    expect(at("Symbols Nerd Font Mono")).toBeGreaterThan(at("Noto Sans Mono"));
    // Color emoji win over the monochrome emoji of symbol fonts.
    expect(at("Noto Color Emoji")).toBeLessThan(at("Noto Sans Symbols 2"));
    expect(chain.endsWith("monospace")).toBe(true);
    // No duplicates when the user's font is also a fallback.
    expect(terminalFontFamily("Menlo").match(/Menlo/g)).toHaveLength(1);
    expect(FALLBACK_FONTS).toContain("Symbols Nerd Font Mono");
  });

  it("maps settings to xterm options", () => {
    const o = terminalOptions(settings({ "terminal.cursorShape": "vertical", "terminal.font.size": 200, "terminal.scrollback": 5000 }));
    expect(o.cursorStyle).toBe("bar");
    expect(o.fontSize).toBe(72);
    expect(o.scrollback).toBe(5000);
    expect(o.fontFamily?.startsWith('"JetBrains Mono"')).toBe(true);
    expect(terminalOptions(settings({ "terminal.font.family": "Iosevka" })).fontFamily?.startsWith('"Iosevka"')).toBe(true);
    expect(terminalRenderer(settings({}))).toBe("auto");
    expect(terminalRenderer(settings({ "terminal.renderer": "dom" }))).toBe("dom");
    expect(terminalRenderer(settings({ "terminal.renderer": "canvas" }))).toBe("auto");
  });
});

/** Feed a recorded stream through a real xterm.js parser with Forge's OSC handlers. */
async function replayInto(frames: string[]) {
  const term = new Terminal({ cols: 100, rows: 40, allowProposedApi: true });
  term.loadAddon(new UnicodeGraphemesAddon());
  const seen = { titles: [] as string[], progress: [] as (Progress | null)[] };
  let last = null as Progress | null;
  term.onTitleChange((t) => seen.titles.push(t));
  term.parser.registerOscHandler(9, (data) => {
    const r = parseOsc(9, data, last);
    if (r.kind === "progress") seen.progress.push((last = r.progress));
    return true;
  });
  for (const f of frames) await new Promise<void>((resolve) => term.write(f, resolve));
  return { term, seen, last };
}

describe("recorded Claude Code session", () => {
  it("sets the title and reports progress up to 60 %", async () => {
    const { seen, last, term } = await replayInto(replayFrames("claude").map((f) => f.data));
    expect(seen.titles).toEqual(["✳ Claude Code", "✳ Add Order total"]);
    expect(seen.progress[0]).toEqual({ state: "indeterminate", percent: 0 });
    expect(seen.progress.filter((p) => p?.state === "normal").map((p) => p!.percent)).toContain(30);
    expect(last).toEqual({ state: "normal", percent: 60 });
    expect(term.modes.synchronizedOutputMode).toBe(false);
    term.dispose();
  });

  it("ends in the error or indeterminate state and clears on exit", async () => {
    const error = await replayInto(replayFrames("claude-error").map((f) => f.data));
    expect(error.last).toEqual({ state: "error", percent: 60 });
    const thinking = await replayInto([...replayFrames("claude-thinking").map((f) => f.data)]);
    expect(thinking.last?.state).toBe("indeterminate");
    const exited = await replayInto([...replayFrames("claude").map((f) => f.data), replayExit()]);
    expect(exited.last).toBeNull();
    expect(exited.seen.titles.at(-1)).toBe("");
    error.term.dispose();
    thinking.term.dispose();
    exited.term.dispose();
  });

  it("keeps box lines and the status line aligned", () => {
    const text = replayFrames("claude").map((f) => f.data).join("");
    const lines = text.split(/\r\n|\r|\x1b\[J/);
    const boxRows = lines.filter((l) => /[╭│╰]/.test(l) && /[╮│╯]$/.test(l.replace(/\x1b\[[0-9;:]*m/g, "")));
    expect(boxRows.length).toBeGreaterThan(4);
    const widths = new Set(boxRows.slice(0, 7).map(cellWidth));
    expect(widths.size).toBe(1);
    expect(cellWidth("💰 $0.42")).toBe(8);
    expect(cellWidth("\ue0a0 main")).toBe(6);
  });

  it("lines up in xterm.js with grapheme widths (emoji, braille, Nerd Font glyphs)", async () => {
    const { term } = await replayInto(replayFrames("claude").map((f) => f.data));
    expect(term.unicode.activeVersion).toBe("15-graphemes");
    const buf = term.buffer.active;
    const rows = Array.from({ length: buf.length }, (_, y) => buf.getLine(y)!);
    /** Cell index of the last non-blank cell. */
    const lastCell = (y: number) => {
      const line = rows[y]!;
      for (let x = term.cols - 1; x >= 0; x--) if (line.getCell(x)?.getChars().trim()) return x;
      return -1;
    };
    const boxRows = rows.map((l, y) => ({ y, text: l.translateToString(true) })).filter((r) => /^[╭│╰]/.test(r.text));
    expect(boxRows.length).toBeGreaterThanOrEqual(10);
    // Welcome box (58 cells) and input box (72 cells): right borders in one column each.
    expect(new Set(boxRows.map((r) => lastCell(r.y)))).toEqual(new Set([57, 71]));
    // The status line with powerline separators and emoji ends where `cellWidth` says.
    const texts = rows.map((l) => l.translateToString(true));
    const statusY = Math.max(...texts.map((t, y) => (t.includes("$0.42") ? y : -1)));
    const status = texts[statusY]!;
    expect(status).toContain("💰");
    expect(lastCell(statusY) + 1).toBe(cellWidth(status.trimEnd()));
    term.dispose();
  });
});
