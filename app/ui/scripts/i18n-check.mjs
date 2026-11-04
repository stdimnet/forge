/**
 * `pnpm i18n:check`: localization coverage of the UI.
 *
 * - Keys used in the code (`t("…")`, `<T k="…" />`, `tOr("…", …)`) must exist in the English
 *   catalogs (`crates/forge-i18n/locales/en/*.json`). A missing key fails the check (exit 1).
 * - Keys of the English catalogs that German (`locales/de`) does not translate are counted
 *   per area.
 * - Hard-coded user-visible strings in `.tsx` files are found heuristically (JSX text, the
 *   attributes `title`, `placeholder`, `aria-label`, `label` and balloon calls) and counted per
 *   feature. They do not fail the check yet; `scripts/i18n-allowlist.json` lists strings that
 *   are not text to translate (product names, file names, units).
 *
 * Options: `--list [path-prefix]` prints every hard-coded candidate, `--json` prints the
 * report as JSON, `--missing-de` lists the untranslated keys.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";

const ui = resolve(import.meta.dirname, "..");
const root = resolve(ui, "../..");
const localesDir = join(root, "crates/forge-i18n/locales");
const args = process.argv.slice(2);
const listAt = args.indexOf("--list");
const listPrefix = listAt >= 0 ? (args[listAt + 1] && !args[listAt + 1].startsWith("--") ? args[listAt + 1] : "") : null;
const asJson = args.includes("--json");

function loadLocale(locale) {
  const out = new Map();
  const areas = {};
  for (const f of readdirSync(join(localesDir, locale)).filter((f) => f.endsWith(".json")).sort()) {
    const area = f.replace(/\.json$/, "");
    const data = JSON.parse(readFileSync(join(localesDir, locale, f), "utf8"));
    areas[area] = Object.keys(data);
    for (const [k, v] of Object.entries(data)) out.set(k, v);
  }
  return { messages: out, areas };
}

function walk(dir, files = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (name === "pkg" || name === "node_modules" || name === "mock") continue;
      walk(p, files);
    } else if (/\.(ts|tsx)$/.test(name) && !/\.test\.tsx?$/.test(name) && !name.endsWith(".d.ts")) {
      files.push(p);
    }
  }
  return files;
}

const en = loadLocale("en");
const de = loadLocale("de");
const allow = new Set(JSON.parse(readFileSync(join(ui, "scripts/i18n-allowlist.json"), "utf8")).strings);

const files = walk(join(ui, "src"));
const used = new Map(); // key -> first location
let dynamicKeys = 0;
const candidates = []; // { file, line, text }

const KEY_CALL = /\b(?:t|tr|tOr|hasMessage)\(\s*"([a-z][a-zA-Z0-9_$-]*\.[a-zA-Z0-9_.$-]+)"|<T\s+k="([a-z][a-zA-Z0-9_.$-]+)"/g;
const DYNAMIC_CALL = /\b(?:t|tr|tOr)\(\s*`/g;
const ATTR = /\b(?:title|placeholder|aria-label|label|alt)="([^"{}]*[A-Za-z]{2}[^"{}]*)"/g;
const BALLOON = /\b(?:showBalloon|notify|notifyError|askConfirm)\(\s*"([^"]*[A-Za-z]{2}[^"]*)"/g;
const PROP = /\b(?:title|label|placeholder|message|confirm|tooltip|emptyText):\s*"([A-Z][^"]*[a-z][^"]*)"/g;

function isText(s) {
  const text = s.trim();
  if (text.length < 2 || !/[A-Za-z]{2}/.test(text)) return false;
  if (allow.has(text)) return false;
  // Identifiers, paths, CSS classes, shortcuts and code-like strings.
  if (/^[a-z0-9_.:/#@$-]+$/.test(text)) return false;
  if (/^(Ctrl|Alt|Shift|Cmd|Meta)\+/.test(text)) return false;
  if (/^[\w.-]+\.(php|js|ts|json|toml|ya?ml|xml|md|lua|py|rs|go)$/i.test(text)) return false;
  if (/^&[a-z]+;$/.test(text)) return false;
  return true;
}

for (const file of files) {
  const rel = relative(ui, file);
  const src = readFileSync(file, "utf8");
  const lines = src.split("\n");
  lines.forEach((line, i) => {
    if (/^\s*(\/\/|\*|\/\*)/.test(line)) return;
    for (const m of line.matchAll(KEY_CALL)) {
      const key = m[1] ?? m[2];
      if (!used.has(key)) used.set(key, `${rel}:${i + 1}`);
    }
    dynamicKeys += [...line.matchAll(DYNAMIC_CALL)].length;
  });
  if (!file.endsWith(".tsx")) continue;
  lines.forEach((line, i) => {
    const trimmed = line.trim();
    if (trimmed.startsWith("//") || trimmed.startsWith("*") || trimmed.startsWith("/*") || trimmed.startsWith("import ")) return;
    const add = (text) => isText(text) && candidates.push({ file: rel, line: i + 1, text: text.trim() });
    for (const m of line.matchAll(ATTR)) add(m[1]);
    for (const m of line.matchAll(BALLOON)) add(m[1]);
    for (const m of line.matchAll(PROP)) add(m[1]);
    // JSX text: between a closing `>` and the next `<` or `{`, on one line.
    for (const m of line.matchAll(/>([^<>{}]+)(?=<|\{|$)/g)) {
      const text = m[1];
      // Skip comparisons and arrows in code (`a > b`, `=> x`).
      const before = line.slice(0, m.index);
      if (/(=|\s)$/.test(before) || /^\s*[=&|?]/.test(text) || /[;=(]/.test(text)) continue;
      if (/^[\s.:,…()-]*$/.test(text)) continue;
      add(text);
    }
    // A JSX text line on its own (between tags on other lines).
    if (/^[A-Z][^<>{}=;()]*[a-z.…:!?]$/.test(trimmed) && !/^[A-Z][a-zA-Z]*$/.test(trimmed) && !/\b(const|let|return|type|interface|export|function)\b/.test(trimmed)) {
      add(trimmed);
    }
  });
}

const missingInEn = [...used].filter(([k]) => !en.messages.has(k));
const enKeys = [...en.messages.keys()];
const missingDe = enKeys.filter((k) => !de.messages.has(k));
const extraDe = [...de.messages.keys()].filter((k) => !en.messages.has(k));
const perArea = Object.fromEntries(
  Object.entries(en.areas).map(([area, keys]) => [area, { keys: keys.length, de: keys.filter((k) => de.messages.has(k)).length }]),
);
const byFeature = new Map();
for (const c of candidates) {
  const parts = c.file.split("/");
  const feature = parts[1] === "features" ? `features/${parts[2]}` : parts.slice(1, 2).join("/");
  byFeature.set(feature, (byFeature.get(feature) ?? 0) + 1);
}
const literalUses = used.size;
const report = {
  keysUsedInUi: literalUses,
  dynamicKeyCalls: dynamicKeys,
  englishKeys: enKeys.length,
  germanKeys: enKeys.length - missingDe.length,
  germanCoverage: enKeys.length ? +(((enKeys.length - missingDe.length) / enKeys.length) * 100).toFixed(1) : 100,
  hardCodedCandidates: candidates.length,
  uiExtraction: +((literalUses / (literalUses + candidates.length)) * 100).toFixed(1),
  areas: perArea,
  missingInEnglish: missingInEn.map(([k, at]) => ({ key: k, at })),
  germanKeysNotInEnglish: extraDe,
};

if (asJson) {
  console.log(JSON.stringify({ ...report, candidatesByFeature: Object.fromEntries(byFeature) }, null, 2));
} else {
  console.log("Localization coverage");
  console.log(`  catalog keys (en):          ${report.englishKeys}`);
  console.log(`  translated to German:       ${report.germanKeys} (${report.germanCoverage}%)`);
  for (const [area, a] of Object.entries(perArea)) {
    console.log(`    ${area.padEnd(10)} ${String(a.de).padStart(5)} / ${String(a.keys).padEnd(5)} ${a.keys ? ((a.de / a.keys) * 100).toFixed(0) : 100}%`);
  }
  console.log(`  keys used in the UI code:   ${literalUses} (+${dynamicKeys} dynamic t() calls)`);
  console.log(`  hard-coded TSX strings:     ${candidates.length} (heuristic; ${report.uiExtraction}% of UI strings go through t())`);
  const top = [...byFeature].sort((a, b) => b[1] - a[1]).slice(0, 15);
  console.log("  most hard-coded strings:   " + top.map(([f, n]) => `${f} ${n}`).join(", "));
  if (listPrefix !== null) {
    for (const c of candidates.filter((c) => c.file.startsWith(listPrefix))) console.log(`${c.file}:${c.line}: ${c.text}`);
  }
  if (args.includes("--missing-de")) for (const k of missingDe) console.log(`missing de: ${k}`);
  for (const k of extraDe) console.log(`warning: de key not in en: ${k}`);
  for (const { key, at } of report.missingInEnglish) console.error(`error: ${at}: key "${key}" is not in the English catalog`);
}
process.exit(missingInEn.length ? 1 : 0);
