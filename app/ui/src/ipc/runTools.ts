/**
 * Run Anything, launch plans and External Tools (Rust: `crates/forge-runtools`,
 * `commands/runtools.rs`). Suggestions, command resolution, compound / before-launch plans,
 * macro expansion and tool validation are computed in Rust; browser mode runs the same code
 * as WebAssembly (`mock/runToolsMock.ts`).
 */
import { invoke } from "./index";
import type { RunConfiguration } from "./run";

/** Something that ran from Run Anything. */
export interface HistoryEntry {
  command: string;
  /** `command` is a run configuration's name. */
  configuration?: boolean;
  /** Working directory relative to the project. */
  cwd?: string;
}

export type RunAnythingAction =
  | { kind: "configuration"; name: string }
  | { kind: "command"; command: string; cwd?: string }
  | { kind: "insert"; text: string };

export interface RunAnythingItem {
  label: string;
  detail?: string;
  /** `recent`, `configuration`, `npm`, `composer`, `php`, `command`, `help` or a provider id. */
  icon: string;
  /** Configuration type of configuration items. */
  typeId?: string;
  matches: number[];
  action: RunAnythingAction;
}

export interface RunAnythingGroup {
  id: string;
  title: string;
  items: RunAnythingItem[];
  more: boolean;
}

export interface SuggestResponse {
  groups: RunAnythingGroup[];
  /** The query starts with `?`. */
  help: boolean;
}

export interface ProviderCommands {
  id: string;
  title: string;
  commands: { command: string; description?: string }[];
}

export interface SuggestRequest {
  query: string;
  recent: HistoryEntry[];
  configurations: { name: string; type: string; typeName?: string }[];
  providers: ProviderCommands[];
  laravel?: boolean;
  perGroup?: number;
}

export interface PlannedTask {
  type: string;
  value: string;
  label: string;
  /** Run another configuration and wait for it. */
  configuration?: string;
  /** npm / Composer / Artisan / shell tasks: the temporary configuration that runs them. */
  run?: RunConfiguration;
}

export interface LaunchPlan {
  runs: { configuration: string; tasks: PlannedTask[] }[];
  compound: boolean;
  allowMultipleInstances: boolean;
}

/** An External Tool (setting `tools.externalTools`, see `features/externalTools/model.ts`). */
export interface ExternalTool {
  name: string;
  group: string;
  description: string;
  program: string;
  arguments: string;
  workingDirectory: string;
  synchronize: boolean;
  openConsole: boolean;
  activateOnStdout: boolean;
  activateOnStderr: boolean;
  outputFilters: string[];
  enabled: boolean;
}

/** What macros expand to; `filePath` may be project-relative. */
export interface MacroContext {
  filePath?: string;
  fileEncoding?: string;
  selectedText?: string;
  line?: number;
  column?: number;
  selectionStartLine?: number;
  selectionStartColumn?: number;
  selectionEndLine?: number;
  selectionEndColumn?: number;
  clipboard?: string;
  prompts?: string[];
}

export interface MacroInfo {
  /** `$FilePath$`. */
  name: string;
  description: string;
  preview?: string;
}

export interface ToolCommand {
  program: string;
  args: string[];
  cwd: string;
  outputFilters: string[];
  commandLine: string;
  /** Macros without a value here. */
  missing?: string[];
}

export const runToolsApi = {
  suggest: (request: SuggestRequest) => invoke<SuggestResponse>("run_anything_suggest", { request }),
  resolve: (command: string, cwd?: string) => invoke<RunConfiguration>("run_anything_resolve", { command, cwd }),
  history: () => invoke<HistoryEntry[]>("run_anything_history"),
  addHistory: (entry: HistoryEntry) => invoke<HistoryEntry[]>("run_anything_history_add", { entry }),
  plan: (configurations: RunConfiguration[], target: string) =>
    invoke<LaunchPlan>("run_launch_plan", { configurations, target }),
  macros: (context: MacroContext) => invoke<MacroInfo[]>("external_tool_macros", { context }),
  command: (tool: ExternalTool, context: MacroContext) => invoke<ToolCommand>("external_tool_command", { tool, context }),
  validate: (tool: ExternalTool, others: ExternalTool[]) => invoke<string[]>("external_tool_validate", { tool, others }),
};
