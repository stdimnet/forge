/**
 * CForge (`app/src-tauri/src/commands/cpp.rs`, logic in `packs/cforge/*`): toolchains, CMake
 * profiles and presets, the CMake model (cmake-file-api), build commands and build problems,
 * CMake Application / test launch requests, clangd and clang-format. Everything is computed
 * in Rust.
 */
import type { LaunchRequest } from "@/features/debug/dapApi";
import { invoke, listen } from "./index";
import type { CommandLine, RunConfiguration } from "./run";

export type CompilerFamily = "gcc" | "clang" | "appleClang" | "msvc" | "unknown";

/** `cforge_toolchains::CompilerInfo`. */
export interface CompilerInfo {
  family: CompilerFamily;
  version: string;
  target: string;
  threadModel?: string;
  installedDir?: string;
  vendor?: string;
}

export interface ToolInfo {
  path: string;
  version: string;
}

export type ToolchainKind = "system" | "minGw" | "visualStudio" | "cross" | "custom";

export interface Debugger {
  kind: "gdb" | "lldb";
  path: string;
  version: string;
}

/** `cforge_toolchains::Toolchain`. */
export interface Toolchain {
  id: string;
  name: string;
  kind: ToolchainKind;
  cCompiler?: string;
  cxxCompiler?: string;
  compiler?: CompilerInfo;
  cmake?: ToolInfo;
  ninja?: ToolInfo;
  make?: ToolInfo;
  debugger?: Debugger;
  environmentScript?: string;
  arch?: string;
  /** `detected` or `settings`. */
  source: string;
  problems?: string[];
}

/** One table of the `cpp.toolchains` setting. */
export interface ToolchainEntry {
  name: string;
  kind: string;
  cCompiler: string;
  cxxCompiler: string;
  cmake: string;
  make: string;
  ninja: string;
  debugger: string;
  environmentScript: string;
}

/** `cforge_cmake::Profile`. */
export interface Profile {
  name: string;
  buildType: string;
  toolchain: string;
  generator: string;
  cmakeOptions: string;
  buildDirectory: string;
  buildOptions: string;
  environment: string[];
  enabled: boolean;
  /** The configure preset it comes from (read-only). */
  preset?: string;
  cacheVariables?: Record<string, string>;
}

/** One table of the `cmake.profiles` setting (strings only). */
export interface ProfileEntry {
  name: string;
  buildType: string;
  toolchain: string;
  generator: string;
  cmakeOptions: string;
  buildDirectory: string;
  buildOptions: string;
  environment: string;
  enabled: string;
}

export interface ProfilesInfo {
  profiles: Profile[];
  active: string;
  presetFiles: string[];
  presetErrors: string[];
}

export type TargetKind = "executable" | "staticLibrary" | "sharedLibrary" | "moduleLibrary" | "objectLibrary" | "interfaceLibrary" | "utility";

export interface CMakeLocation {
  file: string;
  /** 1-based. */
  line: number;
  command?: string;
}

export interface CMakeTarget {
  name: string;
  id: string;
  kind: TargetKind;
  artifacts?: string[];
  sourceDir: string;
  buildDir: string;
  sources: { path: string; language?: string; generated?: boolean }[];
  dependencies?: string[];
  definedAt?: CMakeLocation;
  project: string;
  generatorProvided?: boolean;
}

export interface CMakeModel {
  cmakeVersion: string;
  generator: string;
  multiConfig: boolean;
  sourceDir: string;
  buildDir: string;
  configurations: { name: string; projects: string[]; targets: CMakeTarget[] }[];
  toolchains: { language: string; compilerId: string; path: string; version: string }[];
  cmakeFiles: string[];
}

export interface ConfigureMessage {
  severity: "error" | "warning" | "deprecation";
  file?: string;
  /** 1-based. */
  line?: number;
  command?: string;
  message: string;
  /** 0-based line of the header in the log. */
  logLine: number;
}

export type CMakeStatus = "noCMake" | "notLoaded" | "loading" | "loaded" | "failed";

export interface CMakeState {
  root: string;
  profile: string;
  buildDir: string;
  status: CMakeStatus;
  model?: CMakeModel;
  messages: ConfigureMessage[];
  log: string[];
  commandLine?: string;
}

export type BuildAction = "build" | "rebuild" | "clean";

/** A compiler or linker message of a build (`commands::cpp::BuildProblemView`). */
export interface BuildProblem {
  /** Project-relative, absolute outside the project, or null. */
  path?: string | null;
  file: string;
  /** 1-based; 0 when unknown. */
  line: number;
  column: number;
  severity: "error" | "warning" | "note";
  message: string;
  code?: string;
  tool: string;
  notes?: BuildProblem[];
}

export interface ClangdInfo {
  path?: string | null;
  version?: string | null;
  /** `setting`, `toolchain`, `path`, `managed` or empty. */
  source: string;
  pinnedVersion?: string | null;
}

export interface CompileCommandsSummary {
  files: number;
  projectFiles: number;
  compilers: string[];
  standards: string[];
}

export interface CppStatus {
  clangd: ClangdInfo;
  clangFormat?: string | null;
  toolchain?: Toolchain | null;
  /** `cmake`, `compileCommands` or `sources`. */
  projectKind: "cmake" | "compileCommands" | "sources";
  compileCommandsDir?: string | null;
  compileCommands?: CompileCommandsSummary | null;
  remote: boolean;
}

export interface CMakeCompletion {
  from: number;
  items: { label: string; kind: "command" | "keyword" | "variable"; detail?: string; apply: string }[];
}

export type CppInstallEvent = { message: string } | { label: string; done: number; total?: number | null };

export const cppApi = {
  toolchains: (refresh = false) => invoke<Toolchain[]>("cpp_toolchains", { refresh }),
  inspectToolchain: (entry: ToolchainEntry) => invoke<Toolchain>("cpp_toolchain_inspect", { entry }),
  profiles: () => invoke<ProfilesInfo>("cmake_profiles"),
  state: () => invoke<CMakeState>("cmake_state"),
  reload: (resetCache = false) => invoke<CMakeState>("cmake_reload", { resetCache }),
  cancel: () => invoke<void>("cmake_cancel"),
  buildCommand: (action: BuildAction, targets?: string[]) => invoke<CommandLine>("cmake_build_command", { action, targets }),
  buildProblems: (output: string) => invoke<BuildProblem[]>("cpp_build_problems", { output }),
  targetOf: (configuration: RunConfiguration) => invoke<string | null>("cpp_target_of", { configuration }),
  debugRequest: (configuration: RunConfiguration) => invoke<LaunchRequest>("cpp_debug_request", { configuration }),
  status: (refresh = false) => invoke<CppStatus>("cpp_status", { refresh }),
  installClangd: () => invoke<ClangdInfo>("cpp_clangd_install"),
  format: (path: string, content: string) => invoke<string | null>("cpp_format", { path, content }),
  complete: (text: string, offset: number) => invoke<CMakeCompletion | null>("cmake_complete", { text, offset }),
  onLog: (cb: (e: { line: string; stream: string }) => void) => listen<{ line: string; stream: string }>("cmake://log", cb),
  onState: (cb: (s: CMakeState) => void) => listen<CMakeState>("cmake://state", cb),
  onInstall: (cb: (e: CppInstallEvent) => void) => listen<CppInstallEvent>("cpp://install", cb),
};
