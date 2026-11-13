/**
 * Inspections (Rust: `crates/forge-inspections`, commands in `commands/inspections.rs`):
 * the registry of the enabled plugins, profiles, Inspect Code / Run Inspection by Name with
 * progress and cancel, quick fixes from the results, Code Cleanup and the exports. The
 * profile tree, the editor's diagnostics filter, suppressions and the results tree run
 * synchronously through WebAssembly (`features/inspections/model.ts`).
 */
import { invoke, listen } from "./index";
import type { ScopeFilter } from "./scopes";

export type EditorSeverity = "error" | "warning" | "weak" | "typo" | "info";

export interface SeverityDef {
  /** The level profiles store (`WEAK WARNING`). */
  id: string;
  name: string;
  editor: EditorSeverity;
  custom?: boolean;
}

export interface InspectionOption {
  id: string;
  label: string;
  type: "bool" | "int" | "string" | "enum";
  default: unknown;
  choices?: { value: string; label: string }[];
  min?: number;
  max?: number;
}

export interface InspectionSpec {
  id: string;
  name: string;
  /** `PHP/Data flow`. */
  group: string;
  language?: string;
  severity: string;
  enabled: boolean;
  description?: string;
  options?: InspectionOption[];
  cleanup?: boolean;
  external?: boolean;
  batch?: boolean;
}

export interface ToolMapping {
  source: string;
  code?: string;
  inspection: string;
}

export interface Registry {
  inspections: InspectionSpec[];
  mappings: ToolMapping[];
}

export interface ScopeSettings {
  scope: string;
  enabled: boolean;
  severity?: string;
}

export interface ToolSettings {
  enabled?: boolean;
  severity?: string;
  scopes?: ScopeSettings[];
  options?: Record<string, unknown>;
}

export type ProfileLevel = "ide" | "project";

export interface Profile {
  name: string;
  level: ProfileLevel;
  tools: Record<string, ToolSettings>;
}

export interface ProfilesState {
  profiles: Profile[];
  selected: string;
  hasProject: boolean;
  severities: SeverityDef[];
}

export type ToolOp =
  | { op: "setEnabled"; ids: string[]; enabled: boolean }
  | { op: "setSeverity"; ids: string[]; severity: string }
  | { op: "setScopes"; id: string; scopes: ScopeSettings[] }
  | { op: "setOption"; id: string; option: string; value: unknown }
  | { op: "reset"; ids: string[] };

export type ProfileOp =
  | { op: "copy"; from: string; name: string; level: ProfileLevel }
  | { op: "rename"; from: string; name: string }
  | { op: "delete"; name: string }
  | { op: "reset"; name: string }
  | { op: "select"; name: string }
  | { op: "add"; profile: Profile }
  | { op: "edit"; name: string; edit: ToolOp }
  | { op: "setSeverities"; severities: SeverityDef[] };

/** A row of the Settings tree (`forge_inspections::profile::ViewGroup`). */
export interface ViewGroup {
  name: string;
  path: string;
  check: "all" | "some" | "none";
  modified: boolean;
  ids: string[];
  groups: ViewGroup[];
  inspections: ViewLeaf[];
}

export interface ViewLeaf {
  id: string;
  name: string;
  enabled: boolean;
  severity?: string | null;
  modified: boolean;
  scoped: boolean;
}

/** What Inspect Code covers (`InspectScope`). */
export interface InspectScope {
  kind: "project" | "module" | "directory" | "file" | "files" | "scope";
  path?: string;
  paths?: string[];
  filter?: ScopeFilter;
  includeTests?: boolean;
  label: string;
}

export interface InspectRequest {
  runId: number;
  scope: InspectScope;
  profile?: string;
  only?: string[];
}

export interface FileEdit {
  path: string;
  start: number;
  end: number;
  text: string;
}

export interface Fix {
  title: string;
  edits: FileEdit[];
  safe?: boolean;
}

export interface Location {
  path: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
}

export interface ReportProblem {
  inspection: string;
  inspectionName: string;
  group: string;
  path: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  start: number;
  end: number;
  message: string;
  severity: string;
  severityName: string;
  editor: EditorSeverity;
  snippet: string;
  fixes?: Fix[];
  related?: Location[];
  stamp: number;
  unnecessary?: boolean;
  deprecated?: boolean;
}

export interface Fragment {
  path: string;
  start: number;
  end: number;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
}

export interface DuplicateGroup {
  tokens: number;
  lines: number;
  fragments: Fragment[];
}

export interface Report {
  problems: ReportProblem[];
  files: number;
  profile: string;
  scope: string;
  cancelled: boolean;
  errors: string[];
  duplicates: DuplicateGroup[];
}

export type Grouping = "severity" | "group" | "inspection" | "directory" | "file";

export interface ResultNode {
  key: string;
  kind: Grouping | "problem";
  label: string;
  count: number;
  editor: EditorSeverity;
  problem?: number;
  detail?: string;
  children: ResultNode[];
}

export interface CleanupFile {
  path: string;
  stamp: number;
  before: string;
  after: string;
  fixes: string[];
}

export interface CleanupPlan {
  files: CleanupFile[];
  skipped: number;
}

export interface Applied {
  written: string[];
  skipped: string[];
}

export interface InspectProgress {
  runId: number;
  done: number;
  total: number;
  text: string;
}

export interface SuppressAction {
  kind: "line" | "statement" | "function" | "class" | "file";
  title: string;
  /** UTF-16 offsets. */
  changes: { start: number; end: number; text: string }[];
}

export const inspectionsApi = {
  registry: () => invoke<{ registry: Registry; severities: SeverityDef[] }>("inspections_registry"),
  profiles: () => invoke<ProfilesState>("inspections_profiles"),
  save: (profiles: ProfilesState) => invoke<ProfilesState>("inspections_save", { profiles }),
  exportProfile: (profile: Profile, format: "xml" | "json", path?: string) =>
    invoke<string>("inspections_export_profile", { profile, format, path }),
  importProfile: (args: { text?: string; path?: string }) => invoke<Profile>("inspections_import_profile", args),
  run: (request: InspectRequest) => invoke<Report>("inspections_run", { request }),
  cancel: (runId: number) => invoke<void>("inspections_cancel", { runId }),
  cleanupPlan: (request: InspectRequest) => invoke<CleanupPlan>("inspections_cleanup_plan", { request }),
  cleanupApply: (files: { path: string; stamp: number; after: string }[]) =>
    invoke<Applied>("inspections_cleanup_apply", { files }),
  applyFix: (path: string, stamp: number, fix: Fix) => invoke<Applied>("inspections_apply_fix", { path, stamp, fix }),
  exportReport: (report: Report, format: "html" | "xml" | "json", path?: string) =>
    invoke<string>("inspections_export_report", { report, format, path }),
  onProgress: (cb: (p: InspectProgress) => void) => listen<InspectProgress>("inspections://progress", cb),
  /** The profiles were saved (in any window). */
  onChanged: (cb: (p: ProfilesState) => void) => listen<ProfilesState>("inspections://changed", cb),
};
