/**
 * PHPStorm metadata, JetBrains attributes, include paths and source roots
 * (`commands/php_meta.rs`, computed by the PHP index where the project lives): Forge's PHP
 * inspections and completion, the `#[Language]` places of the language injections,
 * "External Libraries" and Settings | Directories. Offsets are UTF-16.
 */
import { invoke } from "./index";

export interface PhpProblemFix {
  title: string;
  edits: { start: number; end: number; newText: string }[];
}

export interface PhpProblem {
  start: number;
  end: number;
  severity: "error" | "warning" | "weak";
  message: string;
  /** `deprecated`, `expectedValues`, `immutable`, `pureFunction`, `unreachable`, `psr4Namespace`. */
  code: string;
  deprecated?: boolean;
  unnecessary?: boolean;
  fix?: PhpProblemFix;
}

export type PhpCompletionKind = "value" | "constant" | "key" | "class";

export interface PhpCompletionItem {
  label: string;
  insert: string;
  kind: PhpCompletionKind;
  detail?: string;
}

export interface PhpCompletion {
  from: number;
  to: number;
  items: PhpCompletionItem[];
}

/** A parameter marked `#[Language]`: arguments of calls to `name` carry `language`. */
export interface LanguagePlace {
  name: string;
  argument: number;
  method: boolean;
  language: string;
}

/** An include path ("External Libraries"). */
export interface LibraryRoot {
  name: string;
  /** Absolute path on the project's machine. */
  path: string;
  exists: boolean;
}

export interface LibraryNode {
  name: string;
  path: string;
  dir: boolean;
  /** `library://file/…` path the editor opens (files). */
  open?: string;
}

/** A source root of Settings | Directories. */
export interface SourceRootEntry {
  path: string;
  prefix: string;
  /** `source`, `test`, or `none` for an unmarked composer.json root. */
  kind: "source" | "test" | "none";
  /** `composer` (composer.json) or `user`. */
  origin: "composer" | "user";
  overridden: boolean;
}

/** An entry of the `php.sourceRoots` setting. */
export interface SourceRootSetting {
  path: string;
  prefix: string;
  kind: "source" | "test" | "none";
}

export const phpMetaApi = {
  inspect: (path: string, content?: string) => invoke<PhpProblem[]>("php_inspect", { path, content }),
  complete: (path: string, offset: number, content?: string) =>
    invoke<PhpCompletion | null>("php_complete", { path, offset, content }),
  languagePlaces: () => invoke<LanguagePlace[]>("php_language_places"),
  libraries: () => invoke<LibraryRoot[]>("php_libraries"),
  libraryList: (path: string) => invoke<LibraryNode[]>("php_library_list", { path }),
  /** `overrides`: unsaved changes of Settings | Directories (else the stored setting). */
  sourceRoots: (overrides?: SourceRootSetting[]) => invoke<SourceRootEntry[]>("php_source_roots", { overrides }),
};
