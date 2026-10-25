/** Typed client for the Laravel commands (`app/src-tauri/src/commands/laravel.rs`). */
import { invoke } from "./index";

/** 0-based line, UTF-16 character, like LSP positions. */
export interface LaravelLocation {
  path: string;
  line: number;
  character: number;
}

export interface LaravelInfo {
  detected: boolean;
  version?: string;
  routes: number;
  views: number;
  components: number;
  models: number;
  configKeys: number;
  translations: number;
  /** Custom Blade directives without `@`. */
  directives: string[];
}

export interface LaravelRoute {
  methods: string[];
  uri: string;
  name?: string;
  /** `App\Http\Controllers\OrderController@show`, `Closure`, `view: welcome`, ... */
  action: string;
  controller?: string;
  controllerMethod?: string;
  middleware: string[];
  domain?: string;
  source?: LaravelLocation;
  actionLocation?: LaravelLocation;
}

export interface RoutesResult {
  routes: LaravelRoute[];
  source: "static" | "artisan";
}

export type LaravelCompletionKind =
  | "route"
  | "view"
  | "config"
  | "env"
  | "translation"
  | "column"
  | "relation"
  | "property"
  | "method"
  | "component"
  | "directive"
  | "value";

export interface LaravelCompletionItem {
  label: string;
  kind: LaravelCompletionKind;
  detail?: string;
  documentation?: string;
  insertText?: string;
}

/** Items replacing `from..to` (UTF-16 offsets) of the buffer. */
export interface LaravelCompletions {
  from: number;
  to: number;
  items: LaravelCompletionItem[];
}

export interface GutterMark {
  line: number;
  kind: "route" | "controller" | "view";
  tooltip: string;
  targets: LaravelLocation[];
}

export interface LaravelLogFile {
  path: string;
  size: number;
  /** Seconds since the epoch. */
  modified: number;
}

export interface LogStackFrame {
  /** As written in the log. */
  file?: string;
  /** The file in the project. */
  path?: string;
  /** 1-based. */
  line?: number;
  call: string;
  vendor?: boolean;
}

export interface LogException {
  class: string;
  message: string;
  code?: string;
  frame: LogStackFrame;
}

export type LogLevel = "debug" | "info" | "notice" | "warning" | "error" | "critical" | "alert" | "emergency";

export interface LaravelLogEntry {
  time: string;
  channel: string;
  level: LogLevel;
  message: string;
  context?: string;
  exception?: LogException;
  frames?: LogStackFrame[];
  /** 0-based line in the file. */
  line: number;
}

export interface LaravelLogPage {
  path: string;
  files: LaravelLogFile[];
  entries: LaravelLogEntry[];
  size: number;
  truncated?: boolean;
}

export interface LaravelModelSummary {
  name: string;
  class: string;
  table: string;
  attributes: number;
  relations: number;
  location: LaravelLocation;
}

export interface LaravelAppCommand {
  name: string;
  signature: string;
  description?: string;
  location: LaravelLocation;
}

export interface CodeGenerationFile {
  appNamespace: string;
  name: string;
  templateType: string;
  templatePath: string;
}

export interface CodeGeneration {
  id: string;
  name: string;
  classSuffix?: string;
  files: CodeGenerationFile[];
  source: string;
}

export interface LaravelOverview {
  version?: string;
  php?: string;
  appName?: string;
  environment?: string;
  debug?: boolean;
  url?: string;
  routes: number;
  views: number;
  components: number;
  translations: number;
  configKeys: number;
  models: LaravelModelSummary[];
  commands: LaravelAppCommand[];
  logFiles: LaravelLogFile[];
  ideJson: { files: string[]; completions: number; codeGenerations: CodeGeneration[]; errors?: string[] };
}

export interface GeneratedFile {
  path: string;
  content: string;
  /** Already there; not overwritten. */
  exists?: boolean;
}

export const laravelApi = {
  detect: () => invoke<LaravelInfo>("laravel_detect"),
  routes: () => invoke<RoutesResult>("laravel_routes"),
  complete: (path: string, offset: number, content: string) =>
    invoke<LaravelCompletions | null>("laravel_complete", { path, offset, content }),
  goto: (path: string, offset: number, content: string) =>
    invoke<LaravelLocation[]>("laravel_goto", { path, offset, content }),
  gutter: (path: string, content: string) => invoke<GutterMark[]>("laravel_gutter", { path, content }),
  overview: () => invoke<LaravelOverview>("laravel_overview"),
  logs: (path?: string) => invoke<LaravelLogPage>("laravel_logs", { path }),
  generate: (id: string, input: string, write: boolean) => invoke<GeneratedFile[]>("laravel_generate", { id, input, write }),
};
