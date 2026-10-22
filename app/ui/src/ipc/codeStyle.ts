/**
 * Settings | Editor | Code Style (`app/src-tauri/src/commands/code_style.rs`,
 * `crates/forge-codestyle`): the options of every page, the schemes, previews, import/export
 * and the pass Reformat Code runs after the formatter.
 */
import { invoke } from "./index";

export type CodeStyleValue = boolean | number | string;
/** Option values by `<language>.<option>` (`php.indent`, `general.hardWrapAt`). */
export type CodeStyleValues = Record<string, CodeStyleValue>;

export interface CodeStyleChoice {
  value: string;
  label: string;
}

export type CodeStyleOption = {
  /** Key inside the language (`spaces.afterComma`). */
  key: string;
  title: string;
  /** Tab id (`tabsAndIndents`, `spaces`, …). */
  tab: string;
  section?: string;
  default: CodeStyleValue;
  description?: string;
} & (
  | { type: "bool" }
  | { type: "int"; min: number; max: number }
  | { type: "choice"; choices: CodeStyleChoice[] }
  | { type: "text" }
);

export interface CodeStylePreset {
  id: string;
  name: string;
  /** Option keys (without the language) and values. */
  values: Record<string, CodeStyleValue>;
}

export interface CodeStyleLanguage {
  id: string;
  name: string;
  fileTypes: string[];
  tabs: { id: string; title: string }[];
  options: CodeStyleOption[];
  presets: CodeStylePreset[];
  /** File name for the preview's syntax highlighting. */
  samplePath: string;
}

export interface CodeStyleModel {
  general: CodeStyleLanguage;
  languages: CodeStyleLanguage[];
}

export type SchemeLevel = "ide" | "project";

export interface CodeStyleScheme {
  /** `default`, `project` or the id of another IDE scheme. */
  id: string;
  name: string;
  level: SchemeLevel;
  /** Every option's value. */
  values: CodeStyleValues;
}

export interface CodeStyleSchemes {
  schemes: CodeStyleScheme[];
  selected: string;
  hasProject: boolean;
}

export type CodeStyleExportFormat = "json" | "xml";

export interface ImportedScheme {
  name: string;
  values: CodeStyleValues;
  /** How many options the file set. */
  applied: number;
}

export interface PostFormatRequest {
  /** File type id (`php`, `scss`). */
  fileType: string;
  /** The text before formatting (formatter markers). */
  original: string;
  /** The formatted text. */
  text: string;
}

export const codeStyleApi = {
  model: () => invoke<CodeStyleModel>("code_style_model"),
  schemes: () => invoke<CodeStyleSchemes>("code_style_schemes"),
  /** Store the edited schemes; returns them as stored. */
  save: (schemes: CodeStyleSchemes) => invoke<CodeStyleSchemes>("code_style_save", { schemes }),
  preview: (language: string, values: CodeStyleValues) =>
    invoke<string | null>("code_style_preview", { language, values }),
  /** The exported text; with `path` (desktop save dialog) it is also written there. */
  export: (name: string, values: CodeStyleValues, format: CodeStyleExportFormat, path?: string) =>
    invoke<string>("code_style_export", { name, values, format, path: path ?? null }),
  import: (source: { text?: string; path?: string }) =>
    invoke<ImportedScheme>("code_style_import", { text: source.text ?? null, path: source.path ?? null }),
  postFormat: (request: PostFormatRequest) => invoke<string>("code_style_post_format", { request }),
};
