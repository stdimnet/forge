/**
 * PHP quality tools IPC (app/src-tauri/src/commands/php_quality_tools.rs, crate
 * phpforge-quality-tools): PHPStan, Psalm, PHP CS Fixer, PHP_CodeSniffer, PHP Mess Detector,
 * Laravel Pint and Rector. Every command runs project code and is refused in Safe Mode.
 */
import { invoke } from "./index";

export type QualityTool = "phpstan" | "psalm" | "phpCsFixer" | "phpcs" | "phpmd" | "pint" | "rector";

export type ToolKind = "analyzer" | "fixer";

export type SeverityChoice = "tool" | "error" | "warning" | "weak";

export interface ToolConfig {
  tool: QualityTool;
  inspection: boolean;
  path: string;
  fixerPath: string;
  /** "" = run directly, "project" = the project interpreter, else an interpreter name. */
  interpreter: string;
  config: string;
  level: string;
  options: string;
  timeoutSecs: number;
  severity: SeverityChoice;
}

export interface QualityToolInfo {
  tool: QualityTool;
  label: string;
  kind: ToolKind;
  /** "phpcbf", "PHP CS Fixer", "Pint", "Rector" for tools that fix. */
  fixer: string | null;
  configFiles: string[];
  /** Where the executable was found (project-relative when inside it). */
  path: string | null;
  fixerPath: string | null;
  configFile: string | null;
  config: ToolConfig;
}

export interface QualityToolsInfo {
  tools: QualityToolInfo[];
  interpreters: string[];
}

export interface Validation {
  ok: boolean;
  message: string;
  commandLine: string;
}

export type ProblemSeverity = "error" | "warning" | "weak" | "info";

/** A problem a tool reported (0-based lines and columns). */
export interface QualityProblem {
  tool: QualityTool;
  file: string;
  line: number;
  column: number;
  /** Missing: to the end of `line`. */
  endLine?: number;
  endColumn?: number;
  severity: ProblemSeverity;
  message: string;
  rule?: string;
  fixable: boolean;
  link?: string;
}

export interface QualityToolOutcome {
  tool: QualityTool;
  label: string;
  status: "ok" | "missing" | "failed";
  message: string;
}

export interface CheckResult {
  /** The checked files (empty: the whole project). */
  files: string[];
  /** The tools that ran: their problems replace their earlier ones. */
  tools: QualityTool[];
  problems: QualityProblem[];
  outcomes: QualityToolOutcome[];
}

export interface FixResult {
  tool: QualityTool | null;
  text: string;
  changed: boolean;
  /** PHP CS Fixer's and Rector's diff. */
  diff: string;
  rules: string[];
}

export const TOOL_LABELS: Record<QualityTool, string> = {
  phpstan: "PHPStan",
  psalm: "Psalm",
  phpCsFixer: "PHP CS Fixer",
  phpcs: "PHP_CodeSniffer",
  phpmd: "PHP Mess Detector",
  pint: "Laravel Pint",
  rector: "Rector",
};

export const qualityToolsApi = {
  info: () => invoke<QualityToolsInfo>("quality_tools_info"),
  validate: (config: ToolConfig) => invoke<Validation>("quality_tools_validate", { config }),
  /** `text`: the editor text when it may differ from the saved file. */
  check: (path: string, text: string | null) => invoke<CheckResult>("quality_tools_check", { path, text }),
  inspect: (paths: string[] | null) => invoke<CheckResult>("quality_tools_inspect", { paths }),
  fix: (path: string, tool: QualityTool, text: string) => invoke<FixResult>("quality_tools_fix", { path, tool, text }),
};
