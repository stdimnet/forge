/**
 * PHP test frameworks (Rust: `phpforge-test-frameworks`, `commands/test_frameworks.rs`):
 * Settings | PHP | Test Frameworks, the tests of a file for the gutter, Rerun Failed Tests,
 * Behat step definitions and the Test History of the Run tool window.
 */
import { invoke } from "./index";
import type { RunConfiguration, TestNodeData, TestSummary } from "./run";

export type FrameworkId = "phpunit" | "pest" | "codeception" | "behat" | "phpspec";

/** One entry of Settings | PHP | Test Frameworks (`php.testFrameworks`). */
export interface FrameworkConfig {
  framework: FrameworkId;
  /** CLI interpreter name; empty = Local. */
  interpreter: string;
  /** `composer`: `path` is vendor/autoload.php; `path`: the executable or phar. */
  mode: "composer" | "path";
  path: string;
  configFile: string;
  bootstrapFile: string;
  /** Found in the project, not configured. */
  detected?: boolean;
}

export type TargetKind = "class" | "method" | "file" | "describe" | "test" | "dataset" | "feature" | "scenario" | "example";

/** A test of a file (gutter ▶ icon). */
export interface TestTarget {
  /** 0-based line. */
  line: number;
  kind: TargetKind;
  label: string;
  /** Result key (`path` or `path::test`); empty: no result is shown. */
  key: string;
  configuration: RunConfiguration;
}

export interface TestFile {
  framework?: FrameworkId | null;
  targets: TestTarget[];
  /** Runs every test of the file. */
  file?: RunConfiguration;
}

export interface FailedTest {
  name: string;
  member?: string;
  path?: string;
}

/** A Behat step definition (a context method). */
export interface StepDefinition {
  keyword: string;
  pattern: string;
  path: string;
  /** 0-based. */
  line: number;
  column: number;
  method: string;
  class?: string;
}

export interface TestRunEntry {
  id: string;
  name: string;
  type: string;
  /** Milliseconds since the epoch. */
  at: number;
  summary: string;
  counts: TestSummary["counts"];
}

export interface StoredRun {
  entry: TestRunEntry;
  configuration: RunConfiguration;
  nodes: TestNodeData[];
  summary: TestSummary;
}

export const FRAMEWORKS: { id: FrameworkId; label: string; bin: string }[] = [
  { id: "phpunit", label: "PHPUnit", bin: "phpunit" },
  { id: "pest", label: "Pest", bin: "pest" },
  { id: "codeception", label: "Codeception", bin: "codecept" },
  { id: "behat", label: "Behat", bin: "behat" },
  { id: "phpspec", label: "PHPSpec", bin: "phpspec" },
];

export const frameworkLabel = (id: string) => FRAMEWORKS.find((f) => f.id === id)?.label ?? id;

export const testFrameworksApi = {
  list: () => invoke<FrameworkConfig[]>("test_frameworks_list"),
  discover: (path: string, text: string) => invoke<TestFile>("test_discover", { path, text }),
  rerunFailed: (configuration: RunConfiguration, failed: FailedTest[]) =>
    invoke<RunConfiguration | null>("test_rerun_failed", { configuration, failed }),
  stepDefinitions: (text: string, line: number) => invoke<StepDefinition[]>("behat_step_definitions", { text, line }),
  history: () => invoke<TestRunEntry[]>("test_history_list"),
  openHistory: (id: string) => invoke<StoredRun>("test_history_open", { id }),
};
