/** Typed client for the WordPress backend (`app/src-tauri/src/commands/wordpress.rs`). */
import { invoke } from "./index";
import type { UsagesAnswer } from "./codeintel";

/** Project file position; `line` and `column` are 0-based. */
export interface WpLocation {
  path: string;
  line: number;
  column?: number;
}

export type HookKind = "action" | "filter";

export interface WpHeader {
  name: string;
  version?: string;
  textDomain?: string;
  requiresWp?: string;
  requiresPhp?: string;
}

export interface WordPressInfo {
  kind: "site" | "composer" | "content" | "plugin" | "theme";
  version?: string;
  header?: WpHeader;
  headerFile?: string;
  /** WordPress core files are in the project (their hooks are indexed). */
  core: boolean;
  /** `vendor/php-stubs/wordpress-stubs/wordpress-stubs.php` when installed. */
  stubs?: string;
  wpCli?: { path: string; source: "project" | "path" };
  hooks: number;
  files: number;
}

export interface HookSummary {
  name: string;
  kind: HookKind;
  dynamic?: boolean;
  registrations: number;
  invocations: number;
  location: WpLocation;
  /** Forge's summary of a WordPress core hook. */
  summary?: string;
}

export interface HookCompletionItem {
  label: string;
  kind: HookKind;
  detail: string;
  info?: string;
  core?: boolean;
}

export interface HookCompletionList {
  /** UTF-16 offsets of the text the completion replaces. */
  from: number;
  to: number;
  items: HookCompletionItem[];
}

export interface HookGutterMark {
  line: number;
  kind: "registration" | "invocation";
  hook: string;
  title: string;
  targets?: WpLocation[];
}

export interface YodaProblem {
  /** UTF-16 offsets. */
  from: number;
  to: number;
  message: string;
  code: string;
  fixTitle: string;
  replacement: string;
}

/** The plugin id of the WordPress plugin (Settings | Plugins). */
export const WORDPRESS_PLUGIN_ID = "forge.wordpress";

export const wordpressApi = {
  detect: () => invoke<WordPressInfo | null>("wordpress_detect"),
  refresh: () => invoke<WordPressInfo | null>("wordpress_refresh"),
  hooks: () => invoke<HookSummary[]>("wordpress_hooks"),
  complete: (path: string, content: string, offset: number) =>
    invoke<HookCompletionList | null>("wordpress_complete", { path, content, offset }),
  goto: (path: string, content: string, offset: number) => invoke<WpLocation[]>("wordpress_goto", { path, content, offset }),
  gutter: (path: string, content: string) => invoke<HookGutterMark[]>("wordpress_gutter", { path, content }),
  usages: (path: string, content: string, offset: number) =>
    invoke<UsagesAnswer | null>("wordpress_usages", { path, content, offset }),
  inspect: (path: string, content: string) => invoke<YodaProblem[]>("wordpress_inspect", { path, content }),
};
