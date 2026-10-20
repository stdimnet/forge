/** Typed client for the Symfony backend (`app/src-tauri/src/commands/symfony.rs`). */
import { invoke, listen } from "./index";

/** Project file position; `line` and `column` are 0-based. */
export interface SourceLocation {
  path: string;
  line: number;
  column?: number;
}

export interface SymfonyInfo {
  version?: string;
  environment: string;
  consolePath: string;
  hasConsole: boolean;
  containerXml?: string;
  kernelClass?: string;
  routes: number;
  services: number;
  templates: number;
  commands: number;
}

export interface SymfonyRoute {
  name: string;
  path: string;
  /** Empty means any method. */
  methods?: string[];
  /** `App\Controller\OrderController::show`. */
  controller?: string;
  host?: string;
  source: "attribute" | "annotation" | "yaml";
  location?: SourceLocation;
}

export interface SymfonyService {
  id: string;
  class?: string;
  alias?: string;
  public: boolean;
  autowire: boolean;
  autoconfigure: boolean;
  tags?: string[];
  source: "yaml" | "resource" | "container";
  location?: SourceLocation;
}

export interface SymfonyParameter {
  name: string;
  value: string;
  location?: SourceLocation;
}

export interface SymfonyTemplate {
  name: string;
  path: string;
}

export interface ConsoleCommand {
  name: string;
  description?: string;
  aliases?: string[];
  class?: string;
  location?: SourceLocation;
  hidden?: boolean;
  builtin?: boolean;
}

export type SymfonyCompletionKind =
  | "route"
  | "service"
  | "template"
  | "parameter"
  | "tag"
  | "filter"
  | "function"
  | "test"
  | "variable";

export interface SymfonyCompletionItem {
  label: string;
  kind: SymfonyCompletionKind;
  detail?: string;
  info?: string;
  apply?: string;
}

export interface SymfonyCompletionList {
  /** UTF-16 offset where the replaced text starts. */
  from: number;
  items: SymfonyCompletionItem[];
}

export interface SymfonyGutterMark {
  line: number;
  kind: "route" | "template" | "controller" | "command";
  title: string;
  targets?: SourceLocation[];
  command?: string;
}

export type ConsoleEvent =
  | { type: "started"; runId: string; commandLine: string }
  | { type: "output"; runId: string; text: string; stderr: boolean }
  | { type: "exit"; runId: string; code: number | null };

export const CONSOLE_EVENT = "symfony://console";

export const symfonyApi = {
  detect: () => invoke<SymfonyInfo | null>("symfony_detect"),
  refresh: () => invoke<SymfonyInfo>("symfony_refresh"),
  routes: () => invoke<SymfonyRoute[]>("symfony_routes"),
  services: () => invoke<SymfonyService[]>("symfony_services"),
  parameters: () => invoke<SymfonyParameter[]>("symfony_parameters"),
  templates: () => invoke<SymfonyTemplate[]>("symfony_templates"),
  consoleCommands: () => invoke<ConsoleCommand[]>("symfony_console_commands"),
  complete: (path: string, content: string, offset: number) =>
    invoke<SymfonyCompletionList | null>("symfony_complete", { path, content, offset }),
  goto: (path: string, content: string, offset: number) =>
    invoke<SourceLocation[]>("symfony_goto", { path, content, offset }),
  gutter: (path: string, content: string) => invoke<SymfonyGutterMark[]>("symfony_gutter", { path, content }),
  runConsole: (command: string, args: string, runId: string) =>
    invoke<void>("symfony_console_run", { command, args, runId }),
  cancelConsole: (runId: string) => invoke<void>("symfony_console_cancel", { runId }),
  onConsole: (cb: (e: ConsoleEvent) => void) => listen<ConsoleEvent>(CONSOLE_EVENT, cb),
};
