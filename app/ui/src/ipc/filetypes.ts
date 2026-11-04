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
  /** The `fileTypes.overrides` setting: types chosen with Override File Type. */
  overrides?: SettingObject[];
}

/** One entry of the `fileTypes.overrides` setting (path relative to the project). */
export interface FileTypeOverride {
  path: string;
  fileType: string;
}

/** The value of an override that makes a file plain text. */
export const PLAIN_TEXT_ID = "text";

/** A change to the overrides (`forge_filetypes::OverrideEdit`). */
export type OverrideEdit =
  | { op: "set"; path: string; fileType: string }
  | { op: "unset"; path: string }
  /** The file or a folder above it was renamed or moved, or deleted (`to: null`). */
  | { op: "moved"; from: string; to: string | null };

/** One file type of the Override File Type list. */
export interface OverrideChoice {
  id: string;
  name: string;
  /** Contributing plugin that is off or unused by the project. */
  inactivePlugin?: string;
}

export interface OverrideChoices {
  choices: OverrideChoice[];
  /** The type the file is overridden to, if any. */
  current: string | null;
  /** Pattern for Associate with File Type (`*.ext` or the file name). */
  pattern: string;
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
  /** Chosen with Override File Type rather than detected. */
  overridden?: boolean;
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
  /** Override File Type: every type for `request.files[0]`, its override and pattern. */
  overrideChoices: (request: DetectRequest) => invoke<OverrideChoices>("filetypes_override_choices", { request }),
  /** The `fileTypes.overrides` value after `edit`. */
  editOverrides: (overrides: SettingObject[], edit: OverrideEdit) =>
    invoke<FileTypeOverride[]>("filetypes_edit_overrides", { request: { overrides, edit } }),
  /** Template languages with configurable delimiters, and the data languages. */
  templateDelimiters: () => invoke<DelimiterCatalog>("filetypes_template_delimiters"),
};
