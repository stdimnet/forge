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

/**
 * Syntax highlighting of Template Data Languages (as in PHPStorm): the language of the
 * content *between* template tags. Which data language a file uses (the
 * `templates.dataLanguages` setting, compound names like `invoice.tex.twig`, else HTML) is
 * decided in Rust (`crates/forge-filetypes`) and arrives as `FileTypeInfo.dataLanguage`.
 */
export interface DataLanguage {
  id: string;
  /** `undefined` = the template parser's built-in HTML, `null` = plain text. */
  parser: StreamParser<unknown> | null | undefined;
  /** The language's own editor support (Lezer grammar). Without it `parser` is used. */
  support?: () => Extension;
}

const p = (parser: StreamParser<unknown>) => parser;

export const DATA_LANGUAGES: DataLanguage[] = [
  { id: "html", parser: undefined, support: () => html() },
  { id: "xml", parser: p(xmlMode), support: () => xml() },
  { id: "latex", parser: p(stex) },
  { id: "css", parser: p(cssMode), support: () => css() },
  { id: "scss", parser: p(sCSS), support: () => sass() },
  { id: "less", parser: p(lessMode), support: () => less() },
  { id: "javascript", parser: p(jsMode), support: () => javascript() },
  { id: "typescript", parser: p(tsMode), support: () => javascript({ typescript: true }) },
  { id: "json", parser: p(jsonMode), support: () => json() },
  { id: "sql", parser: p(mySQL), support: () => sql({ dialect: MySQL }) },
  { id: "yaml", parser: p(yamlMode), support: () => yaml() },
  { id: "ini", parser: p(properties) },
  { id: "shell", parser: p(shell) },
  { id: "go", parser: p(go), support: () => goLang() },
  { id: "text", parser: null },
];

/** The data language with `id`, HTML for unknown ids. */
export const dataLanguage = (id: string | null | undefined): DataLanguage =>
  DATA_LANGUAGES.find((l) => l.id === id) ?? DATA_LANGUAGES[0];

/** The syntax highlighting of a data language, which the template tags are drawn over. */
export function dataLanguageSupport(id: string | null | undefined): Extension {
  const lang = dataLanguage(id);
  if (lang.support) return lang.support();
  return lang.parser ? StreamLanguage.define(lang.parser) : [];
}
