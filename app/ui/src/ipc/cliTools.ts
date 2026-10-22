/**
 * Command Line Tool Support (Rust: `packs/phpforge/cli-tools`, `commands/command_line_tools.rs`):
 * the configured and detected tools, their commands (cached per tool until composer.lock
 * changes), completion of the console input and runs. A run streams `run://<runId>` events
 * like a Run tool window process; stop it with `runApi.stop`.
 */
import { invoke } from "./index";

export type ToolKind = "composer" | "artisan" | "symfony" | "wp-cli" | "drush" | "symfony-based" | "php-script";
export type ShowConsoleIn = "toolWindow" | "popup";

export interface CliTool {
  kind: ToolKind;
  /** Empty: the kind's label. */
  name: string;
  alias: string;
  path: string;
  /** PHP CLI interpreter name; empty = the project default. */
  interpreter: string;
  showConsoleIn: ShowConsoleIn;
  enabled: boolean;
  /** Found by auto-detection, not configured. */
  detected: boolean;
}

export interface ArgumentDef {
  name: string;
  required: boolean;
  array: boolean;
  description: string;
  default?: string;
  values?: string[];
}

export interface OptionDef {
  /** Without `--`. */
  name: string;
  /** Without `-`. */
  shortcuts?: string[];
  acceptValue: boolean;
  valueRequired: boolean;
  multiple: boolean;
  description: string;
  default?: string;
  values?: string[];
  global?: boolean;
}

export interface CommandDef {
  name: string;
  description: string;
  usage?: string[];
  help?: string;
  aliases?: string[];
  hidden?: boolean;
  /** A namespace of subcommands (WP-CLI `plugin`). */
  group?: boolean;
  arguments?: ArgumentDef[];
  options?: OptionDef[];
}

export interface ToolCommands {
  alias: string;
  commands: CommandDef[];
  /** `cache`, `tool` (just listed), `builtin` (the tool could not run) or `none`. */
  source: "cache" | "tool" | "builtin" | "none";
  error?: string;
}

export type CompletionKind = "tool" | "command" | "group" | "option" | "value";

export interface CliCompletionItem {
  label: string;
  /** Replaces the input between `from` and `to`. */
  insert: string;
  kind: CompletionKind;
  detail?: string;
  description?: string;
}

export interface CommandHelp {
  tool: string;
  alias: string;
  command: CommandDef;
  active?: { kind: "argument" | "option"; name: string };
}

export interface CliCompletion {
  /** UTF-16 offsets (string indices). */
  from: number;
  to: number;
  items: CliCompletionItem[];
  help?: CommandHelp;
  hint?: string;
}

export interface CliRunStarted {
  runId: string;
  commandLine: string;
  alias: string;
  tool: string;
  showConsoleIn: ShowConsoleIn;
}

/** Labels of the "Choose Tool to Add" list, as in PHPStorm. */
export const TOOL_KINDS: { kind: ToolKind; label: string; alias: string; path: string }[] = [
  { kind: "composer", label: "Composer", alias: "c", path: "composer" },
  { kind: "artisan", label: "Laravel Artisan", alias: "a", path: "artisan" },
  { kind: "symfony", label: "Symfony Console", alias: "s", path: "bin/console" },
  { kind: "wp-cli", label: "WP-CLI", alias: "wp", path: "wp" },
  { kind: "drush", label: "Drush", alias: "drush", path: "vendor/bin/drush" },
  { kind: "symfony-based", label: "Tool based on Symfony Console", alias: "", path: "" },
  { kind: "php-script", label: "Tool based on PHP script", alias: "", path: "" },
];

export const toolKindLabel = (kind: string) => TOOL_KINDS.find((k) => k.kind === kind)?.label ?? kind;
export const toolName = (t: Pick<CliTool, "name" | "kind">) => t.name.trim() || toolKindLabel(t.kind);

export const cliToolsApi = {
  list: () => invoke<CliTool[]>("cli_tools_list"),
  commands: (alias: string, reload = false) => invoke<ToolCommands>("cli_tools_commands", { alias, reload }),
  complete: (input: string, offset: number) => invoke<CliCompletion>("cli_tools_complete", { input, offset }),
  /** Subscribe to `run://<runId>` first (`runApi.onEvent`). */
  run: (input: string, runId: string) => invoke<CliRunStarted>("cli_tools_run", { input, runId }),
};
