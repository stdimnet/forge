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
  /** "Clover", "LCOV", "Cobertura", "JaCoCo", "Go coverprofile", "Istanbul JSON", "Merged". */
  format: string;
  files: number;
  lines: Counter;
  branches: Counter;
  /** Functions and methods. */
  methods: Counter;
  /** "85% (17/20)". */
  lineText: string;
  branchText: string;
  methodText: string;
  /** The report names the tests of its lines (Show tests covering this line). */
  perTest: boolean;
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
  methods: Counter;
  /** Files with lines that ran. */
  files: Counter;
  lineText: string;
  branchText: string;
  methodText: string;
}

export interface CoverageView {
  suite: CoverageSuite;
  nodes: CoverageNode[];
  /** Project view hints of covered files and folders: "50% files, 62% lines covered". */
  hints: Record<string, string>;
}

/** A coverage gutter mark (1-based line). */
export interface LineMark {
  line: number;
  hits: number;
  status: "covered" | "partial" | "uncovered";
  branches?: [number, number];
  /** "Hits: 3, branches: 1 of 2 covered". */
  hint: string;
  /** The tests that ran the line, when the report names them. */
  tests?: string[];
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
  /** Extra fields of the run configuration form (stored in `options`). */
  fields?: { key: string; label: string; kind?: string; placeholder?: string; default?: string; options?: { value: string; label: string }[] }[];
}

export const testingApi = {
  /** Ids of the manifest test frameworks the project uses. */
  detect: () => invoke<string[]>("test_frameworks_detect"),
  coverageSuites: () => invoke<CoverageSuite[]>("coverage_suites"),
  coverageView: (id?: string) => invoke<CoverageView | null>("coverage_view", { id: id ?? null }),
  coverageFile: (path: string, id?: string) => invoke<LineMark[]>("coverage_file", { path, id: id ?? null }),
  coverageRemove: (id: string) => invoke<void>("coverage_remove", { id }),
  /** Read a coverage report of the project (LCOV, Cobertura, Clover, JaCoCo, Go profile, Istanbul JSON). */
  coverageImport: (path: string) => invoke<CoverageSuite>("coverage_import", { path }),
  /** Show Coverage Data with several suites: a new suite of their merged reports. */
  coverageMerge: (ids: string[]) => invoke<CoverageSuite>("coverage_merge", { ids }),
  /** Generate Coverage Report: the suite as HTML, written to `path` when given (desktop). */
  coverageExportHtml: (id: string, path?: string) => invoke<string>("coverage_export_html", { id, path: path ?? null }),
  /** The target, host names and path mappings a debugger needs for a configuration's "Run on". */
  runTargetInfo: (configuration: RunConfiguration) => invoke<DebugTarget | null>("run_target_info", { configuration }),
};
