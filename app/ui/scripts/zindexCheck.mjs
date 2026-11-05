/**
 * The rule behind `pnpm zindex:check` (CLI: zindex-check.mjs): a z-index above 10 must come
 * from the layer scale, i.e. use a `--z-*` variable of `src/theme/theme.css`. Small values
 * (1 to 10, and negative ones) are local layers inside a component and stay allowed.
 */

/** The largest raw z-index a component may use for local overlap. */
export const LOCAL_MAX = 10;

/** `z-index` declarations in CSS, `zIndex` / `"z-index"` in style objects and assignments. */
const PATTERNS = [
  /(?<![\w-])z-index\s*:\s*([^;}\n]+)/g,
  /["'`]z-index["'`]\s*:\s*([^,}\n]+)/g,
  /\bzIndex\s*(?::|=)\s*([^,;}\n]+)/g,
];

/** Remove comments, keeping line breaks so line numbers stay right. */
function stripComments(text, css) {
  const blank = (m) => m.replace(/[^\n]/g, " ");
  let out = text.replace(/\/\*[\s\S]*?\*\//g, blank);
  if (!css) out = out.replace(/(^|[^:"'`\\])\/\/[^\n]*/g, (m, p) => p + " ".repeat(m.length - p.length));
  return out;
}

/** Whether one z-index value breaks the rule. */
export function isRawLayer(value) {
  const v = value.trim();
  if (v.includes("--z-")) return false;
  const numbers = v.match(/-?\d+/g) ?? [];
  return numbers.some((n) => Number(n) > LOCAL_MAX);
}

/**
 * The violations in one file: `{ line, value }` for every z-index above 10 that does not use
 * a `--z-*` variable.
 * @param {string} text the file's content
 * @param {boolean} css whether the file is a stylesheet
 */
export function zIndexViolations(text, css) {
  const clean = stripComments(text, css);
  const found = [];
  const patterns = css ? PATTERNS.slice(0, 1) : PATTERNS;
  for (const re of patterns) {
    for (const m of clean.matchAll(re)) {
      if (!isRawLayer(m[1])) continue;
      const line = clean.slice(0, m.index).split("\n").length;
      found.push({ line, value: m[1].trim() });
    }
  }
  return found.sort((a, b) => a.line - b.line);
}
