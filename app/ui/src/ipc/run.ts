/**
 * Run/Debug configurations and runs (Rust: `crates/forge-process`, `commands/run.rs`).
 * Configurations persist in `<project>/.forge/runConfigurations.toml` (shared) and
 * `.forge/runConfigurations.local.toml` (local, plus the selected configuration).
 */
import { invoke, listen } from "./index";

export interface BeforeLaunchTask {
  /** `runConfiguration` (by name) or `shell` (a command line). */
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
}

export interface RunStarted {
  runId: string;
  commandLine: string;
}

export type OutputStream = "stdout" | "stderr" | "system";

export type RunEvent =
  | { type: "started"; pid: number; commandLine: string }
  | { type: "output"; stream: OutputStream; text: string }
  | { type: "exited"; code: number | null; signal: number | null };

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
};
