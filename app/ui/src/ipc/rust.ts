/**
 * RustForge (`app/src-tauri/src/commands/rust.rs`, logic in `packs/rustforge/*`): the
 * project's toolchain and tools, the Cargo workspace (members, targets, features,
 * dependencies), Cargo command lines, `fn main` run targets, rustfmt, Clippy problems with
 * fixes, rust-analyzer downloads and debug launch requests. Everything is computed in Rust.
 */
import type { LaunchRequest } from "@/features/debug/dapApi";
import { invoke, listen } from "./index";
import type { CommandLine, RunConfiguration } from "./run";

export interface Toolchain {
  /** `stable-x86_64-unknown-linux-gnu`. */
  name: string;
  /** `stable`, `beta`, `nightly`, `1.85.0` or a custom name. */
  channel: string;
  host?: string;
  path?: string;
  default: boolean;
  active: boolean;
  /** `rustup`, `path` or `custom`. */
  source: string;
}

export type ActiveReason =
  | { kind: "default" }
  | { kind: "file"; path: string }
  | { kind: "directory"; path: string }
  | { kind: "environment" }
  | { kind: "other"; text: string };

export interface ToolchainFile {
  channel: string;
  components?: string[];
  targets?: string[];
  profile?: string;
  path?: string;
}

export type RustToolId = "rust-analyzer" | "rustfmt" | "clippy" | "cargo-nextest" | "cargo-llvm-cov";

export interface RustToolStatus {
  tool: RustToolId;
  label: string;
  path?: string | null;
  /** `component` (rustup), `cargo` (cargo install) or `download` (rust-analyzer). */
  install?: string | null;
  licence: string;
}

export interface RustStatus {
  rustup?: string | null;
  cargo?: string | null;
  toolchains: Toolchain[];
  active?: { name: string; reason: ActiveReason } | null;
  toolchainFile?: { path: string; file?: ToolchainFile | null; error?: string | null } | null;
  rustc?: { release: string; channel: string; host: string; commitHash?: string } | null;
  sysroot?: string | null;
  stdlibSources?: string | null;
  components: string[];
  tools: RustToolStatus[];
  prettyPrinters?: { etc: string; sources?: string; commitHash?: string } | null;
  /** The toolchain chosen in Settings | Rust (empty: rustup's choice). */
  chosen: string;
}

export type TargetKind = "lib" | "bin" | "test" | "bench" | "example" | "buildScript" | "procMacro";

export interface CargoTarget {
  name: string;
  kind: TargetKind;
  srcPath: string;
  requiredFeatures?: string[];
  test: boolean;
  doctest: boolean;
}

export interface CargoFeature {
  name: string;
  enables: string[];
  default: boolean;
}

export interface CargoPackage {
  id: string;
  name: string;
  version: string;
  edition: string;
  manifest: string;
  dir: string;
  targets: CargoTarget[];
  features: CargoFeature[];
  dependencies: { name: string; req: string; kind: string; optional: boolean }[];
}

export interface DependencyNode {
  name: string;
  version: string;
  /** `registry`, `git`, `path` or `` (a member). */
  source: string;
  kind: "normal" | "dev" | "build";
  features?: string[];
  children?: DependencyNode[];
  repeated?: boolean;
}

export interface CargoWorkspace {
  root: string;
  targetDirectory: string;
  members: CargoPackage[];
  trees: DependencyNode[];
}

export interface RunTarget {
  line: number;
  name: string;
  kind: TargetKind;
  configuration: RunConfiguration;
}

export interface RustEdit {
  path: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  text: string;
}

export interface RustFix {
  title: string;
  applicability: string;
  edits: RustEdit[];
}

export interface RustProblem {
  path: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  severity: "error" | "warning" | "info" | "weak";
  message: string;
  code?: string | null;
  source: string;
  rendered?: string;
  fixes?: RustFix[];
}

/** A Cargo tool window action. */
export type CargoAction = "build" | "check" | "clean" | "update" | "test" | "doc" | "clippy" | "run" | "bench";

export const rustApi = {
  status: (refresh = false) => invoke<RustStatus>("rust_status", { refresh }),
  installCommand: (kind: "toolchain" | "component" | "cargoPlugin", name: string, toolchain?: string) =>
    invoke<CommandLine>("rust_install_command", { kind, name, toolchain }),
  downloadAnalyzer: () => invoke<string>("rust_analyzer_download"),
  workspace: (refresh = false) => invoke<CargoWorkspace>("cargo_workspace", { refresh }),
  cargoCommand: (action: CargoAction, pkg?: string, target?: string) =>
    invoke<CommandLine>("cargo_command", { action, package: pkg, target }),
  fileTargets: (path: string, content: string) => invoke<RunTarget[]>("rust_file_targets", { path, content }),
  format: (path: string, content: string) => invoke<string | null>("rust_format", { path, content }),
  clippy: (path?: string) => invoke<RustProblem[]>("rust_clippy", { path }),
  debugRequest: (configuration: RunConfiguration) => invoke<LaunchRequest>("rust_debug_request", { configuration }),
  onInstall: (cb: (e: { message?: string; label?: string; done?: number; total?: number | null }) => void) =>
    listen("rust://install", cb),
  onWorkspace: (cb: (ws: CargoWorkspace) => void) => listen<CargoWorkspace>("rust://workspace", cb),
};
