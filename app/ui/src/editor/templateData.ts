import { StreamLanguage, type StreamParser } from "@codemirror/language";
import type { Extension } from "@codemirror/state";
import { html } from "@codemirror/lang-html";
import { css } from "@codemirror/lang-css";
import { sass } from "@codemirror/lang-sass";
import { less } from "@codemirror/lang-less";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { sql, MySQL } from "@codemirror/lang-sql";
import { yaml } from "@codemirror/lang-yaml";
import { xml } from "@codemirror/lang-xml";
import { go as goLang } from "@codemirror/lang-go";
import { xml as xmlMode } from "@codemirror/legacy-modes/mode/xml";
import { stex } from "@codemirror/legacy-modes/mode/stex";
import { css as cssMode, sCSS, less as lessMode } from "@codemirror/legacy-modes/mode/css";
import { javascript as jsMode, typescript as tsMode, json as jsonMode } from "@codemirror/legacy-modes/mode/javascript";
import { mySQL } from "@codemirror/legacy-modes/mode/sql";
import { yaml as yamlMode } from "@codemirror/legacy-modes/mode/yaml";
import { properties } from "@codemirror/legacy-modes/mode/properties";
import { shell } from "@codemirror/legacy-modes/mode/shell";
import { go } from "@codemirror/legacy-modes/mode/go";
import { matchesGlobList } from "@/features/settings/globs";
import { getSetting } from "@/state/settings";

/**
 * Template Data Languages (as in PHPStorm): the language of the content *between* template
 * tags. Chosen per file from, in order:
 *   1. the `templates.dataLanguages` setting: entries like `legacy/latex/*.tpl=latex`;
 *   2. a compound extension: `invoice.tex.twig`, `query.sql.twig`, `styles.css.tpl`;
 *   3. HTML.
 */
export interface DataLanguage {
  id: string;
  label: string;
  /** `undefined` = the template parser's built-in HTML, `null` = plain text. */
  parser: StreamParser<unknown> | null | undefined;
  /** The language's own editor support (Lezer grammar). Without it `parser` is used. */
  support?: () => Extension;
}

/** The syntax highlighting of a data language, which the template tags are drawn over. */
export function dataLanguageSupport(lang: DataLanguage): Extension {
  if (lang.support) return lang.support();
  return lang.parser ? StreamLanguage.define(lang.parser) : [];
}

const p = (parser: StreamParser<unknown>) => parser;

export const DATA_LANGUAGES: DataLanguage[] = [
  { id: "html", label: "HTML", parser: undefined, support: () => html() },
  { id: "xml", label: "XML", parser: p(xmlMode), support: () => xml() },
  { id: "latex", label: "LaTeX", parser: p(stex) },
  { id: "css", label: "CSS", parser: p(cssMode), support: () => css() },
  { id: "scss", label: "SCSS", parser: p(sCSS), support: () => sass() },
  { id: "less", label: "Less", parser: p(lessMode), support: () => less() },
  { id: "javascript", label: "JavaScript", parser: p(jsMode), support: () => javascript() },
  { id: "typescript", label: "TypeScript", parser: p(tsMode), support: () => javascript({ typescript: true }) },
  { id: "json", label: "JSON", parser: p(jsonMode), support: () => json() },
  { id: "sql", label: "SQL", parser: p(mySQL), support: () => sql({ dialect: MySQL }) },
  { id: "yaml", label: "YAML", parser: p(yamlMode), support: () => yaml() },
  { id: "ini", label: "INI / Properties", parser: p(properties) },
  { id: "shell", label: "Shell Script", parser: p(shell) },
  { id: "go", label: "Go", parser: p(go), support: () => goLang() },
  { id: "text", label: "Plain text", parser: null },
];

/** Inner extensions of compound file names (`*.tex.twig`) → data language id. */
const COMPOUND: Record<string, string> = {
  html: "html", htm: "html", xhtml: "html",
  xml: "xml", svg: "xml", rss: "xml", atom: "xml",
  tex: "latex", latex: "latex", sty: "latex", cls: "latex",
  css: "css", scss: "scss", less: "less",
  js: "javascript", mjs: "javascript", ts: "typescript",
  json: "json", sql: "sql", yml: "yaml", yaml: "yaml",
  ini: "ini", env: "ini", conf: "ini", sh: "shell", go: "go",
  txt: "text", md: "text", csv: "text", eml: "text",
};

const byId = (id: string) => DATA_LANGUAGES.find((l) => l.id === id.trim().toLowerCase());

/** Data language from the file name alone (`report.tex.twig` → LaTeX). */
export function dataLanguageFromName(path: string): DataLanguage | undefined {
  const parts = (path.split("/").pop() ?? path).toLowerCase().split(".");
  // name.<inner>.<template>, e.g. ["invoice", "tex", "twig"]
  if (parts.length < 3) return undefined;
  const inner = COMPOUND[parts[parts.length - 2]];
  return inner ? byId(inner) : undefined;
}

/** Data language from `pattern=language` entries (first match wins). */
export function dataLanguageFromMappings(path: string, mappings: string[]): DataLanguage | undefined {
  for (const entry of mappings) {
    const eq = entry.lastIndexOf("=");
    if (eq <= 0) continue;
    const pattern = entry.slice(0, eq).trim();
    const lang = byId(entry.slice(eq + 1));
    if (lang && matchesGlobList(path, pattern)) return lang;
  }
  return undefined;
}

export function dataLanguageFor(path: string): DataLanguage {
  const mappings = getSetting<string[]>("templates.dataLanguages", []);
  return dataLanguageFromMappings(path, mappings) ?? dataLanguageFromName(path) ?? DATA_LANGUAGES[0];
}
