/**
 * GoForge (`app/src-tauri/src/commands/go.rs`, logic in `packs/goforge/*`): the project's Go
 * SDK and tools, SDK downloads, `go.mod` / `go.work` analysis and completion, module and
 * `go generate` command lines, gofmt / goimports, go vet and golangci-lint, Delve launch
 * requests and pprof profiles. Everything is computed in Rust.
 */
import type { LaunchRequest } from "@/features/debug/dapApi";
import { invoke, listen } from "./index";
import type { CommandLine, RunConfiguration } from "./run";

export interface GoSdk {
  root: string;
  /** `1.24.7`. */
  version: string;
  /** `PATH`, `GOROOT`, `Homebrew`, `Forge`, `~/sdk`, a folder or `custom`. */
  source: string;
}

export type GoToolId = "gopls" | "dlv" | "goimports" | "golangciLint";

export interface GoToolStatus {
  tool: GoToolId;
  label: string;
  path?: string | null;
  /** `module@version` Forge installs with `go install`; absent: Forge does not install it. */
  install?: string | null;
  licence: string;
}

export interface ModuleRoot {
  path: string;
  dir: string;
}

export interface GoStatus {
  sdk?: GoSdk | null;
  custom: boolean;
  sdks: GoSdk[];
  tools: GoToolStatus[];
  modules: ModuleRoot[];
  gopath: string;
}

export interface GoFile {
  filename: string;
  os: string;
  arch: string;
  version: string;
  sha256: string;
  size: number;
  kind: string;
}

export interface GoDownloadable {
  version: string;
  file: GoFile;
}

export interface ModWord {
  text: string;
  line: number;
  start: number;
  end: number;
}

export interface ModProblem {
  line: number;
  start: number;
  end: number;
  message: string;
  severity: "error" | "warning";
}

export interface GoRequire {
  path: ModWord;
  version?: ModWord | null;
  indirect: boolean;
}

export interface GoReplace {
  old: ModWord;
  oldVersion?: ModWord | null;
  new: ModWord;
  newVersion?: ModWord | null;
}

export type ModFile =
  | {
      kind: "mod";
      module?: ModWord | null;
      go?: ModWord | null;
      toolchain?: ModWord | null;
      requires: GoRequire[];
      replaces: GoReplace[];
      excludes: GoRequire[];
      problems: ModProblem[];
    }
  | { kind: "work"; go?: ModWord | null; uses: ModWord[]; replaces: GoReplace[]; problems: ModProblem[] };

export interface ModCompletionItem {
  label: string;
  detail?: string | null;
  kind: "directive" | "module" | "version";
}

export interface ModCompletionResult {
  from: number;
  items: ModCompletionItem[];
}

export interface GenerateDirective {
  line: number;
  command: string;
}

export interface GoProblem {
  path: string;
  line: number;
  column: number;
  severity: "error" | "warning" | "info";
  message: string;
  source: "go vet" | "golangci-lint";
  code: string;
}

export interface DelveStatus {
  path?: string | null;
  install?: string | null;
}

export interface ValueType {
  kind: string;
  unit: string;
}

export interface TopRow {
  name: string;
  file: string;
  line: number;
  flat: number;
  cum: number;
}

export interface CallNode {
  name: string;
  file: string;
  line: number;
  total: number;
  self: number;
  children: CallNode[];
}

export interface ProfileView {
  sampleTypes: ValueType[];
  sampleIndex: number;
  total: number;
  samples: number;
  durationNanos: number;
  periodType?: ValueType | null;
  period: number;
  top: TopRow[];
  tree: CallNode;
  pruned: number;
}

/** `go://install` while an SDK downloads: a step or download progress. */
export type GoInstallEvent = { message: string } | { label: string; done: number; total?: number | null };

export const goApi = {
  status: (refresh = false) => invoke<GoStatus>("go_status", { refresh }),
  sdkReleases: () => invoke<GoDownloadable[]>("go_sdk_releases"),
  /** Download an SDK; call only after the user agreed. */
  sdkDownload: (version: string) => invoke<GoSdk>("go_sdk_download", { version }),
  onInstall: (cb: (e: GoInstallEvent) => void) => listen<GoInstallEvent>("go://install", cb),
  toolInstallCommand: (tool: GoToolId) => invoke<CommandLine>("go_tool_install_command", { tool }),
  modParse: (path: string, content: string) => invoke<ModFile>("go_mod_parse", { path, content }),
  modComplete: (path: string, content: string, offset: number) =>
    invoke<ModCompletionResult | null>("go_mod_complete", { path, content, offset }),
  moduleVersions: (module: string) => invoke<string[]>("go_module_versions", { module }),
  moduleCommand: (action: "tidy" | "download" | "verify" | "workSync" | "get", arg?: string, dir?: string) =>
    invoke<CommandLine>("go_module_command", { action, arg, dir }),
  generateDirectives: (content: string) => invoke<GenerateDirective[]>("go_generate_directives", { content }),
  generateCommand: (path: string, content: string, line?: number) =>
    invoke<CommandLine>("go_generate_command", { path, content, line }),
  format: (path: string, content: string) => invoke<string | null>("go_format", { path, content }),
  lint: (tool: "vet" | "golangci", path?: string) => invoke<GoProblem[]>("go_lint", { tool, path }),
  delveStatus: () => invoke<DelveStatus>("go_delve_status"),
  debugRequest: (configuration: RunConfiguration) => invoke<LaunchRequest>("go_debug_request", { configuration }),
  profilePath: (name: string) => invoke<string>("go_profile_path", { name }),
  pprofOpen: (path: string, sampleIndex?: number) => invoke<ProfileView>("pprof_open", { path, sampleIndex }),
};
