/**
 * File type IPC: mirrors `crates/forge-filetypes` and the `filetypes_detect` command in
 * `app/src-tauri/src/commands/search.rs`; browser mode runs the same Rust code as
 * WebAssembly.
 */
import { invoke } from "./index";

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
}

export const filetypesApi = {
  detect: (request: DetectRequest) => invoke<FileTypeInfo[]>("filetypes_detect", { request }),
};
