/**
 * CForge v2 (`app/src-tauri/src/commands/cpp_tools.rs` and `embedded.rs`, logic in
 * `packs/cforge/analyzers` and `packs/cforge/embedded`): sanitizer reports, Valgrind
 * Memcheck, perf, vcpkg / Conan, PlatformIO, ESP-IDF, Zephyr, OpenOCD and SVD files.
 * Everything is computed in Rust; runs stream their results as `run://<runId>` events
 * (`sanitizer`, `valgrind`, `perf`, see `@/ipc/run`).
 */
import type { LaunchRequest } from "@/features/debug/dapApi";
import { invoke } from "./index";
import type { CMakeCompletion } from "./cpp";
import type { ProfileView } from "./profile";
import type { CommandLine, RunConfiguration } from "./run";

// ---- Stacks of sanitizers and Valgrind --------------------------------------------------------

export interface StackFrame {
  index: number;
  address?: string;
  function: string;
  /** As reported (absolute or relative to the compiler's folder). */
  file?: string;
  /** 1-based; 0 when unknown. */
  line: number;
  column: number;
  module?: string;
  /** The project file, when the frame is in the project. */
  path?: string;
}

export interface Stack {
  title: string;
  frames: StackFrame[];
}

export type SanitizerKind = "address" | "leak" | "thread" | "memory" | "undefined";

export interface SanitizerReport {
  sanitizer: SanitizerKind;
  /** `AddressSanitizer`, … */
  tool: string;
  /** `heap-buffer-overflow`, `data race`, `Direct leak`, `signed integer overflow`. */
  kind: string;
  message: string;
  details?: string[];
  stacks: Stack[];
  summary?: string;
  location?: StackFrame;
}

export interface Leaked {
  bytes: number;
  blocks: number;
}

export interface ValgrindError {
  unique: string;
  thread: number;
  /** `InvalidRead`, `UninitCondition`, `Leak_DefinitelyLost`, … */
  kind: string;
  what: string;
  count: number;
  leaked?: Leaked;
  stacks: Stack[];
  location?: StackFrame;
}

export interface ValgrindReport {
  tool: string;
  command: string;
  pid: number;
  errors: ValgrindError[];
  leakSummary: { definitelyLost: Leaked; indirectlyLost: Leaked; possiblyLost: Leaked; stillReachable: Leaked };
  errorCount: number;
  state: string;
}

/** A profile of "Profile with perf". */
export interface PerfProfile {
  title: string;
  view: ProfileView;
}

export type NativeTool = "valgrind" | "perf";

export interface NativeToolsInfo {
  valgrind?: string | null;
  perf?: string | null;
  valgrindUnsupported?: string | null;
  perfUnsupported?: string | null;
  remote: boolean;
}

// ---- Package managers -------------------------------------------------------------------------

export interface PackageManagers {
  vcpkg?: { root?: string; toolchainFile?: string; dependencies: string[] };
  conan?: { conanfile: string; toolchainFile?: string };
  /** The toolchain file the active profile configures with. */
  toolchainFile?: string;
}

// ---- Embedded ---------------------------------------------------------------------------------

export interface PioEnv {
  name: string;
  platform: string;
  board: string;
  framework: string[];
  default: boolean;
  /** 1-based line of the section in platformio.ini. */
  line: number;
  options: Record<string, string>;
}

export interface PioProject {
  envs: PioEnv[];
  defaultEnvs: string[];
  platformio: Record<string, string>;
  problems?: string[];
}

export type PioTask = "build" | "upload" | "monitor" | "test" | "clean" | "compiledb";
export type IdfAction = "build" | "flash" | "monitor" | "menuconfig" | "fullClean";
export type WestAction = "build" | "rebuild" | "flash" | "debug";

export interface EmbeddedInfo {
  kinds: { platformio: boolean; espIdf: boolean; zephyr: boolean };
  platformio?: PioProject | null;
  espIdf?: { target?: string | null; idfPath?: string | null; port?: string | null } | null;
  zephyr?: { board?: string | null; localBoards: string[]; buildDir: string } | null;
}

export interface EmbeddedLaunch {
  server?: CommandLine | null;
  download?: CommandLine | null;
  request: LaunchRequest;
  /** Absolute SVD file for the Peripherals view. */
  svd?: string | null;
  elf: string;
}

export interface PeripheralSummary {
  name: string;
  description?: string;
  group?: string;
  baseAddress: number;
  registers: number;
}

export interface DeviceSummary {
  name: string;
  vendor?: string;
  description?: string;
  peripherals: PeripheralSummary[];
}

export interface FieldValue {
  name: string;
  description?: string;
  /** `[11:10]`, `[5]`. */
  bits: string;
  access: string;
  value?: number | null;
  /** `Output (1)`, `0x1F (31)`. */
  display: string;
}

export interface RegisterValue {
  name: string;
  description?: string;
  address: number;
  size: number;
  access: string;
  value?: number | null;
  display: string;
  resetValue: number;
  fields: FieldValue[];
}

export const cppToolsApi = {
  sanitizerReports: (output: string) => invoke<SanitizerReport[]>("cpp_sanitizer_reports", { output }),
  nativeTools: () => invoke<NativeToolsInfo>("cpp_native_tools"),
  openValgrind: (path: string) => invoke<ValgrindReport>("cpp_valgrind_open", { path }),
  packages: () => invoke<PackageManagers>("cpp_packages"),
  vcpkgComplete: (text: string, offset: number) => invoke<CMakeCompletion | null>("cpp_vcpkg_complete", { text, offset }),
  conanInstallCommand: () => invoke<CommandLine>("cpp_conan_install_command"),
  restartClangd: () => invoke<void>("cpp_clangd_restart"),
  embeddedInfo: () => invoke<EmbeddedInfo>("embedded_info"),
  pioCommand: (env: string, task: PioTask) => invoke<CommandLine>("pio_command", { env, task }),
  idfCommand: (action: IdfAction) => invoke<CommandLine>("idf_command", { action }),
  westCommand: (action: WestAction, board?: string) => invoke<CommandLine>("west_command", { action, board }),
  embeddedLaunch: (configuration: RunConfiguration) => invoke<EmbeddedLaunch>("cpp_embedded_launch", { configuration }),
  svdPeripherals: (path: string) => invoke<DeviceSummary>("cpp_svd_peripherals", { path }),
  svdRead: (path: string, peripheral: string, sessionId: string) => invoke<RegisterValue[]>("cpp_svd_read", { path, peripheral, sessionId }),
};
