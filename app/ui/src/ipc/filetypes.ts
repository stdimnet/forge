/**
 * File type IPC: mirrors `crates/forge-filetypes` and the `filetypes_detect` command in
 * `app/src-tauri/src/commands/search.rs`; browser mode runs the same Rust code as
 * WebAssembly.
 */
import { invoke } from "./index";
import type { SettingObject } from "./settings";

export interface FileRef {
  path: string;
  /** First line of the content, for `#!` and modeline detection. */
  firstLine?: string;
}

export interface DetectRequest {
  files: FileRef[];
  /** Plugins that are disabled or unused by the project; their file types are skipped. */
  inactivePlugins: string[];
  /** The `templates.dataLanguages` setting. */
  dataLanguages: string[];
  /** The `editor.softWrapFiles` setting, or null when soft wraps are off. */
  softWrapFiles: string | null;
  /** The `templates.delimiters` setting. */
  templateDelimiters: SettingObject[];
  /** The `fileTypes.associations` setting: patterns added to or removed from file types. */
  associations?: SettingObject[];
  /** The `fileTypes.custom` setting: the user's own file types. */
  customTypes?: SettingObject[];
}

/** Highlighting of a custom file type (Settings | Editor | File Types). */
export interface CustomSyntax {
  lineComment: string;
  lineCommentAtStart: boolean;
  blockCommentStart: string;
  blockCommentEnd: string;
  hexPrefix: string;
  numberPostfixes: string;
  /** Keyword lists 1–4 (lower case when `ignoreCase`). */
  keywords: string[][];
  ignoreCase: boolean;
  pairedBraces: boolean;
  pairedBrackets: boolean;
  pairedParens: boolean;
  stringEscapes: boolean;
}

/** One row of Settings | Editor | File Types. */
export interface FileTypeRow {
  id: string;
  name: string;
  description?: string;
  /** Patterns after the user's changes. */
  patterns: string[];
  /** Patterns the user added. */
  added: string[];
  /** Bundled or plugin patterns the user removed. */
  removed: string[];
  plugin?: string;
  custom: boolean;
  syntax?: CustomSyntax;
  indent: number;
}

export interface DataLanguageInfo {
  id: string;
  label: string;
}

export interface FileTypeInfo {
  /** File type id, `null` for plain text. */
  id: string | null;
  /** Status bar label, e.g. "PHP", "Twig (LaTeX)" or "Plain text". */
  label: string;
  /** Indentation unit. */
  indent: string;
  /** Data language of template files. */
  dataLanguage: DataLanguageInfo | null;
  softWrap: boolean;
  /**
   * Delimiters of template files whose template language has configurable ones, by field key
   * (`left`, `variableStart`, …), resolved from the `templates.delimiters` setting.
   */
  delimiters: Record<string, string> | null;
  /** Highlighting of a custom file type. */
  custom?: CustomSyntax;
}

/** One delimiter of a template language, e.g. Twig's variable start `{{`. */
export interface DelimiterField {
  key: string;
  label: string;
  default: string;
}

/** A template language whose delimiters can be changed. */
export interface ConfigurableTemplate {
  /** File type id, as used in the entries' `template` field. */
  id: string;
  label: string;
  /** Pairs of fields: start and end of each tag kind, in display order. */
  fields: DelimiterField[];
}

export interface DelimiterCatalog {
  templates: ConfigurableTemplate[];
  dataLanguages: DataLanguageInfo[];
}

export const filetypesApi = {
  detect: (request: DetectRequest) => invoke<FileTypeInfo[]>("filetypes_detect", { request }),
  /** Settings | Editor | File Types (`files` may be empty). */
  list: (request: DetectRequest) => invoke<FileTypeRow[]>("filetypes_list", { request }),
  /** Template languages with configurable delimiters, and the data languages. */
  templateDelimiters: () => invoke<DelimiterCatalog>("filetypes_template_delimiters"),
};
