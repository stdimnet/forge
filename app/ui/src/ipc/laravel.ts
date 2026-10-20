/** Typed client for the Laravel commands (`app/src-tauri/src/commands/laravel.rs`). */
import { invoke, listen } from "./index";

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
  | "directive";

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

export interface ArtisanCommand {
  name: string;
  description: string;
  usage?: string;
  arguments: string[];
  options: string[];
}

export interface ArtisanCommands {
  commands: ArtisanCommand[];
  /** `false` when artisan could not run and the built-in list is shown. */
  fromProject: boolean;
}

export type ArtisanEvent =
  | { type: "started"; commandLine: string }
  | { type: "output"; text: string; stderr: boolean }
  | { type: "exit"; code: number }
  | { type: "error"; message: string };

export const artisanEvent = (runId: string) => `laravel://artisan/${runId}`;

export const laravelApi = {
  detect: () => invoke<LaravelInfo>("laravel_detect"),
  routes: () => invoke<RoutesResult>("laravel_routes"),
  complete: (path: string, offset: number, content: string) =>
    invoke<LaravelCompletions | null>("laravel_complete", { path, offset, content }),
  goto: (path: string, offset: number, content: string) =>
    invoke<LaravelLocation[]>("laravel_goto", { path, offset, content }),
  gutter: (path: string, content: string) => invoke<GutterMark[]>("laravel_gutter", { path, content }),
  artisanCommands: () => invoke<ArtisanCommands>("laravel_artisan_commands"),
  /** Subscribe with `onArtisan` first; resolves with the exit code. */
  artisanRun: (commandLine: string, runId: string) => invoke<number>("laravel_artisan_run", { commandLine, runId }),
  artisanCancel: (runId: string) => invoke<void>("laravel_artisan_cancel", { runId }),
  onArtisan: (runId: string, cb: (e: ArtisanEvent) => void) => listen<ArtisanEvent>(artisanEvent(runId), cb),
};
