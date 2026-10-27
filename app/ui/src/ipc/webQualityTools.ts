/**
 * Web code quality tools IPC (app/src-tauri/src/commands/web_quality_tools.rs, crate
 * webforge-quality-tools): ESLint, Prettier, Stylelint and Biome from the project's
 * node_modules, run with the Node.js interpreter. Every command runs project code and is
 * refused in Safe Mode (`format` answers null there, so the usual formatter runs).
 */
import { invoke } from "./index";
import type { ProblemSeverity } from "./qualityTools";

export type WebTool = "eslint" | "prettier" | "stylelint" | "biome";

export type WebToolMode = "disabled" | "automatic" | "manual";

export interface WebToolConfig {
  tool: WebTool;
  mode: WebToolMode;
  package: string;
  config: string;
  options: string;
  workingDirs: string;
  files: string;
  onSave: boolean;
  onReformat: boolean;
}

export interface WebToolInfo {
  tool: WebTool;
  label: string;
  /** The npm package (`@biomejs/biome`). */
  package: string;
  configFiles: string[];
  /** The package found in the project (or the manual one). */
  path: string | null;
  version: string | null;
  /** The configuration file found for the project root. */
  configFile: string | null;
  config: WebToolConfig;
}

export interface WebToolsInfo {
  tools: WebToolInfo[];
  /** The Node.js interpreter the tools run with. */
  node: string;
}

export interface WebValidation {
  ok: boolean;
  message: string;
  commandLine: string;
}

/** A problem a tool reported (0-based lines and columns). */
export interface WebProblem {
  tool: WebTool;
  file: string;
  line: number;
  column: number;
  endLine?: number;
  endColumn?: number;
  severity: ProblemSeverity;
  message: string;
  rule?: string;
  fixable: boolean;
  link?: string;
}

export interface WebToolOutcome {
  tool: WebTool;
  label: string;
  status: "ok" | "missing" | "failed";
  message: string;
}

export interface WebCheckResult {
  /** The checked files (empty: the whole project). */
  files: string[];
  /** The tools whose problems the result replaces. */
  tools: WebTool[];
  problems: WebProblem[];
  outcomes: WebToolOutcome[];
}

export interface WebFixResult {
  tool: WebTool | null;
  text: string;
  changed: boolean;
}

export const WEB_TOOL_LABELS: Record<WebTool, string> = {
  eslint: "ESLint",
  prettier: "Prettier",
  stylelint: "Stylelint",
  biome: "Biome",
};

export const webQualityToolsApi = {
  info: () => invoke<WebToolsInfo>("web_quality_tools_info"),
  validate: (config: WebToolConfig) => invoke<WebValidation>("web_quality_tools_validate", { config }),
  /** `text`: the editor text (null: the saved file). */
  check: (path: string, text: string | null) => invoke<WebCheckResult>("web_quality_tools_check", { path, text }),
  inspect: (paths: string[] | null) => invoke<WebCheckResult>("web_quality_tools_inspect", { paths }),
  fix: (path: string, tool: WebTool, text: string) => invoke<WebFixResult>("web_quality_tools_fix", { path, tool, text }),
  /** Reformat Code with Prettier or Biome; null: the usual formatter. */
  format: (path: string, text: string) => invoke<WebFixResult | null>("web_quality_tools_format", { path, text }),
};
