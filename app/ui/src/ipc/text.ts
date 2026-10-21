/**
 * Text files (`crates/forge-text`, `app/src-tauri/src/commands/editor.rs`): reading and
 * writing with encoding and line separator, the on-save cleanup and the effective format of
 * a file (indent, separators, guides). Browser mode runs the same Rust code as WebAssembly.
 */
import { invoke } from "./index";

export type Eol = "\n" | "\r\n" | "\r";
export type EncodingId = "UTF-8" | "UTF-16LE" | "UTF-16BE" | "ISO-8859-1" | "windows-1252";

/** Encodings Forge reads and writes, in the order the status bar popup lists them. */
export const ENCODINGS: EncodingId[] = ["UTF-8", "UTF-16LE", "UTF-16BE", "ISO-8859-1", "windows-1252"];

export interface TextFile {
  /** Content with `\n` separators. */
  text: string;
  eol: Eol;
  encoding: EncodingId;
  bom: boolean;
  /** Bytes invalid in `encoding` were replaced. */
  malformed: boolean;
}

export type StripMode = "none" | "modified" | "all";

export interface SaveOptions {
  stripTrailingSpaces: StripMode;
  ensureNewlineAtEof: boolean;
}

export interface SaveRequest {
  text: string;
  /** Text as last loaded or saved, to find modified lines. */
  saved?: string;
  /** 0-based lines with a caret (never stripped). */
  caretLines: number[];
  options: SaveOptions;
}

export interface SaveResult {
  text: string;
  changed: boolean;
}

export interface IndentStyle {
  useTabs: boolean;
  size: number;
  tabSize: number;
  continuation: number;
}

export type IndentSource = "codeStyle" | "fileType" | "editorConfig" | "detected";

export interface FileFormat {
  indent: IndentStyle;
  indentSource: IndentSource;
  /** Status bar text: "4 spaces", "Tab", "2 spaces*" (detected). */
  indentLabel: string;
  configuredIndent: IndentStyle;
  /** Settings page of the language's code style, e.g. `editor.code-style.php`. */
  codeStylePage: string | null;
  languageName: string | null;
  /** Separator for files without line breaks yet (new files). */
  lineSeparator: Eol;
  /** Separator EditorConfig requires. */
  requiredLineSeparator: Eol | null;
  charset: { encoding: EncodingId; bom: boolean } | null;
  save: SaveOptions;
  hardWrapAt: number;
  visualGuides: number[];
  editorConfig: boolean;
}

export interface FileFormatRequest {
  path: string;
  languageId?: string | null;
  text?: string;
  fileTypeIndent?: string;
}

export interface CommitMessageRules {
  subjectMaxLength: number;
  blankLineAfterSubject: boolean;
  bodyRightMargin: number;
}

export interface CommitMessageProblem {
  kind: "subjectTooLong" | "missingBlankLine" | "bodyLineTooLong";
  line: number;
  from: number;
  to: number;
  message: string;
}

export const eolLabel = (eol: Eol): string => (eol === "\r\n" ? "CRLF" : eol === "\r" ? "CR" : "LF");

export const encodingLabel = (encoding: EncodingId, bom: boolean): string => (bom && encoding === "UTF-8" ? "UTF-8 BOM" : encoding);

export const textApi = {
  read: (path: string, encoding?: EncodingId) => invoke<TextFile>("fs_read_text", { path, encoding }),
  write: (path: string, text: string, eol: Eol, encoding: EncodingId, bom: boolean) =>
    invoke<void>("fs_write_text", { path, text, eol, encoding, bom }),
  prepareSave: (request: SaveRequest) => invoke<SaveResult>("text_prepare_save", { request }),
  fileFormat: (request: FileFormatRequest) => invoke<FileFormat>("editor_file_format", { request }),
  inspectCommitMessage: (message: string, rules: CommitMessageRules) =>
    invoke<CommitMessageProblem[]>("vcs_inspect_commit_message", { message, rules }),
};
