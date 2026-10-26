/**
 * The shared test runner, coverage and run targets (Rust: `crates/forge-testing`,
 * `crates/forge-runtargets`, `commands/testing.rs`). Discovery and Rerun Failed Tests of
 * every framework go through `testFrameworksApi` (`test_discover`, `test_rerun_failed`).
 */
import { invoke } from "./index";
import type { DebugTarget, RunConfiguration } from "./run";

export interface Counter {
  covered: number;
  total: number;
}

/** A coverage suite (a Run with Coverage run or an imported report). */
export interface CoverageSuite {
  id: string;
  name: string;
  /** Milliseconds since the epoch. */
  at: number;
  /** "Clover", "LCOV", "Cobertura", "JaCoCo", "Go coverprofile". */
  format: string;
  files: number;
  lines: Counter;
  branches: Counter;
  /** "85% (17/20)". */
  lineText: string;
  branchText: string;
}

/** A row of the Coverage tool window. */
export interface CoverageNode {
  /** Project-relative path ("" for the project). */
  path: string;
  name: string;
  kind: "project" | "directory" | "file";
  depth: number;
  lines: Counter;
  branches: Counter;
  lineText: string;
  branchText: string;
}

export interface CoverageView {
  suite: CoverageSuite;
  nodes: CoverageNode[];
}

/** A coverage gutter mark (1-based line). */
export interface LineMark {
  line: number;
  hits: number;
  status: "covered" | "partial" | "uncovered";
  branches?: [number, number];
  /** "Hits: 3, branches: 1 of 2 covered". */
  hint: string;
}

/** A test framework a plugin manifest declares (`[[contributions.testFrameworks]]`). */
export interface TestFrameworkSpec {
  id: string;
  name: string;
  description?: string;
  icon?: string;
  language?: string;
  testFiles?: string[];
  program?: string;
  programSetting?: string;
  scopes?: Record<string, string[]>;
  report?: string;
  coverage?: { args?: string[]; file?: string; format?: string };
  debuggable?: boolean;
}

export const testingApi = {
  /** Ids of the manifest test frameworks the project uses. */
  detect: () => invoke<string[]>("test_frameworks_detect"),
  coverageSuites: () => invoke<CoverageSuite[]>("coverage_suites"),
  coverageView: (id?: string) => invoke<CoverageView | null>("coverage_view", { id: id ?? null }),
  coverageFile: (path: string, id?: string) => invoke<LineMark[]>("coverage_file", { path, id: id ?? null }),
  coverageRemove: (id: string) => invoke<void>("coverage_remove", { id }),
  /** Read a coverage report of the project (LCOV, Cobertura, Clover, JaCoCo, Go profile). */
  coverageImport: (path: string) => invoke<CoverageSuite>("coverage_import", { path }),
  /** The target, host names and path mappings a debugger needs for a configuration's "Run on". */
  runTargetInfo: (configuration: RunConfiguration) => invoke<DebugTarget | null>("run_target_info", { configuration }),
};
