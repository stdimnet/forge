/** xterm.js options from the Settings > Tools > Terminal page. */
import type { ITerminalOptions } from "@xterm/xterm";
import type { SettingValue } from "@/ipc/settings";

export type TerminalRenderer = "auto" | "webgl" | "dom";

/**
 * Fonts tried after the chosen one, so powerline / Nerd Font symbols, box drawing, braille
 * and emoji render even when the main font lacks them. Missing fonts are skipped by the
 * browser.
 */
export const FALLBACK_FONTS = [
  // Text: the IDE's monospace fonts (`--font-mono`) and common system ones.
  "JetBrains Mono",
  "Fira Code",
  "Cascadia Code",
  "Menlo",
  "Consolas",
  "DejaVu Sans Mono",
  "Noto Sans Mono",
  // Powerline and Nerd Font icons.
  "Symbols Nerd Font Mono",
  "Symbols Nerd Font",
  "JetBrainsMono Nerd Font Mono",
  "MesloLGS NF",
  "MesloLGM Nerd Font Mono",
  "FiraCode Nerd Font Mono",
  // Color emoji before symbol fonts, which have monochrome versions of some emoji.
  "Noto Color Emoji",
  "Apple Color Emoji",
  "Segoe UI Emoji",
  "Noto Sans Symbols 2",
  "Segoe UI Symbol",
];

const GENERIC = new Set(["monospace", "serif", "sans-serif", "system-ui", "ui-monospace", "emoji"]);

const quote = (f: string) => (GENERIC.has(f) || /^["'].*["']$/.test(f) ? f : `"${f.replace(/"/g, "")}"`);

/** CSS font-family list: the user's fonts (comma separated), then the fallbacks, then monospace. */
export function terminalFontFamily(primary: string): string {
  const user = primary
    .split(",")
    .map((f) => f.trim().replace(/^["']|["']$/g, ""))
    .filter(Boolean);
  const seen = new Set<string>();
  const list = [...user, ...FALLBACK_FONTS, "monospace"].filter((f) => {
    const k = f.toLowerCase();
    if (seen.has(k)) return false;
    seen.add(k);
    return true;
  });
  return list.map(quote).join(", ");
}

type Get = <T extends SettingValue>(key: string, fallback: T) => T;

const num = (v: unknown, fallback: number, min: number, max: number) => {
  const n = typeof v === "number" ? v : Number(v);
  return Number.isFinite(n) ? Math.max(min, Math.min(max, n)) : fallback;
};

/** PHPStorm's "Cursor shape" values to xterm.js cursor styles. */
const CURSOR: Record<string, ITerminalOptions["cursorStyle"]> = {
  block: "block",
  underline: "underline",
  vertical: "bar",
};

/** Options that follow settings; applied on creation and whenever a setting changes. */
export function terminalOptions(get: Get): ITerminalOptions {
  const family = get<string>("terminal.font.family", "").trim() || get<string>("editor.font.family", "JetBrains Mono");
  return {
    fontFamily: terminalFontFamily(family),
    fontSize: num(get("terminal.font.size", 13), 13, 6, 72),
    lineHeight: num(get("terminal.font.lineHeight", 1.2), 1.2, 1, 3),
    cursorStyle: CURSOR[get<string>("terminal.cursorShape", "block")] ?? "block",
    cursorBlink: get<boolean>("terminal.cursorBlink", true),
    scrollback: Math.round(num(get("terminal.scrollback", 10_000), 10_000, 0, 1_000_000)),
    macOptionIsMeta: get<boolean>("terminal.optionAsMeta", true),
  };
}

export function terminalRenderer(get: Get): TerminalRenderer {
  const r = get<string>("terminal.renderer", "auto");
  return r === "webgl" || r === "dom" ? r : "auto";
}
