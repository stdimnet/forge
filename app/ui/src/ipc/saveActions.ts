/**
 * Actions on Save IPC (app/src-tauri/src/commands/save_actions.rs, crates
 * forge-save-actions and phpforge-lang-php). Browser mode answers the planning through
 * WebAssembly and mocks the rest (it never runs tools or writes files).
 */
import { invoke } from "./index";

/** What started a save: Ctrl+S, autosave, or a save that runs no actions (encoding change). */
export type SaveTrigger = "explicit" | "autosave";

export type ReformatScope = "whole" | "changed";

export type SaveTool = "eslint" | "prettier" | "phpCsFixer" | "pint";

export interface SavePlan {
  reformat: ReformatScope | null;
  optimizeImports: boolean;
  rearrange: boolean;
  codeCleanup: boolean;
  /** External tools, in the order they run. */
  tools: SaveTool[];
}

export interface PhpActions {
  optimizeImports: boolean;
  rearrange: boolean;
  codeCleanup: boolean;
}

export interface PhpActionsResult {
  text: string;
  /** Titles of the actions that changed the text. */
  applied: string[];
}

export type ToolStatus = "ok" | "missing" | "failed";

export interface ToolOutcome {
  tool: SaveTool;
  label: string;
  status: ToolStatus;
  message: string;
}

export interface ToolsReport {
  /** The file on disk changed; the editor reloads it. */
  changed: boolean;
  outcomes: ToolOutcome[];
}

export interface FileTypeChoice {
  id: string;
  name: string;
}

export interface ToolLocation {
  tool: SaveTool;
  label: string;
  /** Project-relative (or absolute) path, null when the tool is not installed. */
  path: string | null;
}

export interface SaveActionsInfo {
  fileTypes: FileTypeChoice[];
  tools: ToolLocation[];
}

export const isEmptyPlan = (p: SavePlan): boolean =>
  !p.reformat && !p.optimizeImports && !p.rearrange && !p.codeCleanup && p.tools.length === 0;

export const saveActionsApi = {
  plan: (path: string, trigger: SaveTrigger) => invoke<SavePlan>("save_actions_plan", { path, trigger }),
  changedLines: (path: string, original: string, formatted: string) =>
    invoke<string>("save_actions_changed_lines", { path, original, formatted }),
  php: (text: string, actions: PhpActions) => invoke<PhpActionsResult>("save_actions_php", { text, actions }),
  runTools: (path: string, tools: SaveTool[]) => invoke<ToolsReport>("save_actions_run_tools", { path, tools }),
  info: () => invoke<SaveActionsInfo>("save_actions_info"),
};
