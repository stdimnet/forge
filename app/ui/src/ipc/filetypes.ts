/**
 * File type IPC: mirrors `crates/forge-filetypes` and the `filetypes_detect` command in
 * `app/src-tauri/src/commands/search.rs`; browser mode runs the same Rust code as
 * WebAssembly.
 */
import { invoke } from "./index";
import type { SettingObject } from "./settings";

export interface FileRef {
  path: string;
  /** First line of the content, for `#!` detection. */
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
  /** Template languages with configurable delimiters, and the data languages. */
  templateDelimiters: () => invoke<DelimiterCatalog>("filetypes_template_delimiters"),
};
