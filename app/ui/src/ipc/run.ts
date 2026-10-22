/**
 * Run/Debug configurations and runs (Rust: `crates/forge-process`, `commands/run.rs`).
 * Configurations persist in `<project>/.forge/runConfigurations.toml` (shared) and
 * `.forge/runConfigurations.local.toml` (local, plus the selected configuration).
 */
import { invoke, listen } from "./index";

export interface BeforeLaunchTask {
  /**
   * `runConfiguration` (by name), `externalTool` (`Group/Name`), `browser` (a URL), `npm`
   * (a script), `composer` (a script or command), `artisan` (a command) or `shell` (a
   * command line).
   */
  type: string;
  value: string;
}

export interface RunConfiguration {
  name: string;
  /** Type id, e.g. `phpunit`, `php-script`, `artisan`, `composer`, `shell`, `npm`. */
  type: string;
  storeAsProjectFile?: boolean;
  workingDirectory?: string;
  env?: Record<string, string>;
  /** Interpreter executable; empty = the project default (PHP: setting `php.interpreter`). */
  interpreter?: string;
  interpreterOptions?: string;
  arguments?: string;
  /** Type-specific fields. */
  options?: Record<string, string>;
  beforeLaunch?: BeforeLaunchTask[];
}

export interface RunConfigurations {
  configurations: RunConfiguration[];
  selected?: string;
  /** Per-type defaults for new configurations (one per type). */
  templates?: RunConfiguration[];
}

export interface CommandLine {
  program: string;
  args: string[];
  /** Absolute, or relative to the project root; empty = project root. */
  cwd?: string;
  env?: Record<string, string>;
}

export interface RunRequest {
  /** Chosen by the UI so it can subscribe to `run://<runId>` before starting. */
  runId?: string;
  /** Built by the backend's configuration kind registry... */
  configuration?: RunConfiguration;
  /** ...or a command line built by a UI-side configuration type. */
  command?: CommandLine;
  /** Extra environment, e.g. from a debug launcher. */
  env?: Record<string, string>;
  /** Extra arguments right after the program (e.g. `-dxdebug.mode=debug` for PHP). */
  interpreterArgs?: string[];
  /** Build a test tree from TeamCity service messages (`tests` events). */
  testRunner?: boolean;
  /** External Tools output filters (`$FILE_PATH$:$LINE$`) that link file references. */
  outputFilters?: string[];
}

export interface RunStarted {
  runId: string;
  commandLine: string;
}

export type OutputStream = "stdout" | "stderr" | "system";

/** Styles of console text, computed in Rust from ANSI escapes; colors are CSS values. */
export interface AnsiStyle {
  fg?: string;
  bg?: string;
  bold?: boolean;
  dim?: boolean;
  italic?: boolean;
  underline?: boolean;
  inverse?: boolean;
}

/** A project file (`target` is workspace-relative, line/column 1-based) or a URL. */
export interface SpanLink {
  url?: boolean;
  target: string;
  line?: number;
  column?: number;
}

export interface ConsoleSpan {
  text: string;
  style: AnsiStyle;
  link?: SpanLink;
}

export interface ConsoleLine {
  segments: ConsoleSpan[];
}

/** Console update: with `replaceLast` the first line replaces the last line shown. */
export interface ConsoleDelta {
  replaceLast: boolean;
  lines: ConsoleLine[];
  /** Lines to remove from the start afterwards (the console cycle buffer is full). */
  dropFront?: number;
}

export type TestStatus = "running" | "passed" | "failed" | "ignored" | "pending";

export interface TestLocation {
  /** File path as reported (absolute or relative). */
  path: string;
  /** The path inside the project, when it is in it. */
  workspacePath?: string;
  /** Class or method name after `::`. */
  member?: string;
  line?: number;
}

/** A test tree node as computed by the backend (`forge_output::TestNode`). */
export interface TestNodeData {
  id: number;
  parentId?: number;
  name: string;
  kind: "root" | "suite" | "test";
  /** Suites derive their status from their children. */
  status: TestStatus;
  /** Milliseconds. */
  duration?: number;
  /** "1 s 245 ms". */
  durationText: string;
  location?: TestLocation;
  message?: string;
  details?: string;
  expected?: string;
  actual?: string;
  /** Console of a test: its output and the failure. */
  console: ConsoleLine[];
}

export interface TestSummary {
  /** "Tests failed: 1, passed: 3 of 4 tests – 51 ms". */
  text: string;
  failed: boolean;
  done: number;
  total: number;
  counts: { total: number; passed: number; failed: number; ignored: number; running: number };
  expectedCount?: number;
}

export type RunEvent =
  | { type: "started"; pid: number; commandLine: string }
  | { type: "output"; stream: OutputStream; text: string }
  | { type: "exited"; code: number | null; signal: number | null }
  | ({ type: "console" } & ConsoleDelta)
  | { type: "tests"; nodes: TestNodeData[]; summary: TestSummary };

/** A file reference in terminal output; offsets are string indices. */
export interface FileLink {
  start: number;
  end: number;
  path: string;
  /** Workspace-relative path. */
  target?: string;
  line?: number;
  column?: number;
}

export const runEvent = (runId: string) => `run://${runId}`;

export const runApi = {
  loadConfigurations: () => invoke<RunConfigurations>("run_configurations_load"),
  saveConfigurations: (configurations: RunConfigurations) =>
    invoke<void>("run_configurations_save", { configurations }),
  commandLine: (request: RunRequest) => invoke<CommandLine>("run_command_line", { request }),
  start: (request: RunRequest) => invoke<RunStarted>("run_start", { request }),
  stop: (runId: string) => invoke<void>("run_stop", { runId }),
  write: (runId: string, data: string) => invoke<void>("run_write", { runId, data }),
  dispose: (runId: string) => invoke<void>("run_dispose", { runId }),
  onEvent: (runId: string, cb: (e: RunEvent) => void) => listen<RunEvent>(runEvent(runId), cb),
  /** File links of a line of terminal output that resolve into the project. */
  fileLinks: (text: string) => invoke<FileLink[]>("output_file_links", { text }),
};
