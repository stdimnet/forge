/**
 * WCAG 2.x contrast ratios of the theme tokens in `theme.css`, for the contrast check
 * (`contrast.test.ts`, `pnpm contrast`). Pure functions without DOM, so Node and vitest share
 * them.
 */

export type Rgb = [number, number, number];
export type Rgba = [number, number, number, number];

/** Parse `#rgb`, `#rrggbb`, `#rrggbbaa` or `rgb()/rgba()`; null for anything else. */
export function parseColor(text: string): Rgba | null {
  const s = text.trim().toLowerCase();
  let m = /^#([0-9a-f]{3})$/.exec(s);
  if (m) return [...[...m[1]].map((c) => parseInt(c + c, 16)), 1] as Rgba;
  m = /^#([0-9a-f]{6})([0-9a-f]{2})?$/.exec(s);
  if (m) {
    const n = m[1];
    const a = m[2] ? parseInt(m[2], 16) / 255 : 1;
    return [parseInt(n.slice(0, 2), 16), parseInt(n.slice(2, 4), 16), parseInt(n.slice(4, 6), 16), a];
  }
  m = /^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*(?:,\s*([\d.]+)\s*)?\)$/.exec(s);
  if (m) return [Number(m[1]), Number(m[2]), Number(m[3]), m[4] === undefined ? 1 : Number(m[4])];
  return null;
}

/** A translucent color painted over an opaque one. */
export function composite(fg: Rgba, bg: Rgb): Rgb {
  const a = fg[3];
  return [0, 1, 2].map((i) => Math.round(fg[i] * a + bg[i] * (1 - a))) as Rgb;
}

/** Relative luminance (WCAG 2.x). */
export function luminance([r, g, b]: Rgb): number {
  const lin = (c: number) => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/** Contrast ratio of two opaque colors, 1 to 21. */
export function contrastRatio(a: Rgb, b: Rgb): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/**
 * Token values per theme from the theme CSS. A block whose selector list contains `:root`
 * alone applies to the default theme ("dark"); `:root[data-theme="x"]` to theme x. Later
 * blocks override earlier ones, as in the browser.
 */
export function themeTokens(css: string): Map<string, Map<string, string>> {
  const out = new Map<string, Map<string, string>>();
  const text = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const block = /([^{}]+)\{([^{}]*)\}/g;
  for (let m = block.exec(text); m; m = block.exec(text)) {
    const selectors = m[1].split(",").map((s) => s.trim());
    const themes = new Set<string>();
    for (const sel of selectors) {
      if (sel === ":root") themes.add("dark");
      const t = /^:root\[data-theme="([^"]+)"\]$/.exec(sel);
      if (t) themes.add(t[1]);
    }
    if (!themes.size) continue;
    const decls = [...m[2].matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)];
    for (const theme of themes) {
      const map = out.get(theme) ?? new Map<string, string>();
      for (const [, name, value] of decls) map.set(name, value.trim());
      out.set(theme, map);
    }
  }
  return out;
}

/** What a pair must reach: text (4.5 AA / 7 AAA) or large text and UI parts (3 / 4.5). */
export type PairKind = "text" | "ui";

export interface TokenPair {
  fg: string;
  bg: string;
  kind: PairKind;
}

/** Minimum ratio of a pair kind at a level. */
export const required = (kind: PairKind, level: "AA" | "AAA"): number =>
  kind === "text" ? (level === "AA" ? 4.5 : 7) : level === "AA" ? 3 : 4.5;

/** Backgrounds text is drawn on. */
export const TEXT_BACKGROUNDS = ["--bg-app", "--bg-toolbar", "--bg-panel", "--bg-editor", "--bg-elevated", "--bg-input"];

/** Text colors used for content that must be readable (not disabled or decorative). */
export const TEXT_TOKENS = ["--fg", "--fg-strong", "--fg-muted"];

/** Syntax token colors, checked on the editor background. */
export const SYNTAX_TOKENS = [
  "--syn-keyword",
  "--syn-string",
  "--syn-number",
  "--syn-comment",
  "--syn-variable",
  "--syn-function",
  "--syn-type",
  "--syn-attribute",
  "--syn-operator",
  "--syn-tag",
];

/**
 * The pairs a theme is checked on. Secondary text (`--fg-faint`: hints, line numbers,
 * placeholders) needs 3:1 in the normal themes and 7:1 in the high-contrast ones, which also
 * need visible borders.
 */
export function themePairs(level: "AA" | "AAA" = "AA"): TokenPair[] {
  const pairs: TokenPair[] = [];
  for (const fg of TEXT_TOKENS) for (const bg of TEXT_BACKGROUNDS) pairs.push({ fg, bg, kind: "text" });
  for (const fg of SYNTAX_TOKENS) pairs.push({ fg, bg: "--bg-editor", kind: "text" });
  // Hints, line numbers and placeholders: secondary text.
  for (const bg of ["--bg-panel", "--bg-editor"]) pairs.push({ fg: "--fg-faint", bg, kind: level === "AAA" ? "text" : "ui" });
  // Text on the selection and hover rows, and the primary button.
  pairs.push({ fg: "--fg", bg: "--bg-selection", kind: "text" });
  pairs.push({ fg: "--fg", bg: "--bg-hover", kind: "text" });
  pairs.push({ fg: "--fg-strong", bg: "--bg-active", kind: "text" });
  pairs.push({ fg: "--accent-fg", bg: "--accent", kind: "text" });
  // Status colors as text (problems, VCS file names).
  for (const fg of ["--danger", "--warning", "--success", "--info", "--vcs-modified", "--vcs-added", "--vcs-untracked"])
    pairs.push({ fg, bg: "--bg-panel", kind: "text" });
  // UI parts: input borders, the focus ring (accent) and the accent as an icon color.
  if (level === "AAA") {
    pairs.push({ fg: "--border-strong", bg: "--bg-panel", kind: "ui" });
    pairs.push({ fg: "--border", bg: "--bg-panel", kind: "ui" });
  }
  pairs.push({ fg: "--accent", bg: "--bg-panel", kind: "ui" });
  pairs.push({ fg: "--accent", bg: "--bg-editor", kind: "ui" });
  return pairs;
}

export interface PairResult extends TokenPair {
  ratio: number;
  min: number;
  ok: boolean;
}

/** Resolve a token (following `var(--x)`) to an opaque color over `base`. */
function resolve(tokens: Map<string, string>, name: string, base: Rgb, depth = 0): Rgb | null {
  const raw = tokens.get(name);
  if (!raw || depth > 4) return null;
  const ref = /^var\((--[\w-]+)\)$/.exec(raw);
  if (ref) return resolve(tokens, ref[1], base, depth + 1);
  const c = parseColor(raw);
  return c ? composite(c, base) : null;
}

/** Check the pairs of one theme at a level; pairs with unknown tokens are skipped. */
export function checkTheme(tokens: Map<string, string>, level: "AA" | "AAA", pairs = themePairs(level)): PairResult[] {
  const out: PairResult[] = [];
  const editor = resolve(tokens, "--bg-editor", [0, 0, 0]) ?? [0, 0, 0];
  for (const p of pairs) {
    const bg = resolve(tokens, p.bg, editor);
    const fg = bg && resolve(tokens, p.fg, bg);
    if (!bg || !fg) continue;
    const ratio = contrastRatio(fg, bg);
    const min = required(p.kind, level);
    out.push({ ...p, ratio, min, ok: ratio >= min - 0.005 });
  }
  return out;
}

/** The level each theme must meet: AAA for the high-contrast themes, AA for the others. */
export const levelOf = (theme: string): "AA" | "AAA" => (theme.startsWith("high-contrast") ? "AAA" : "AA");
