/**
 * KotlinForge IPC (app/src-tauri/src/commands/kotlin.rs; crate packs/kotlinforge/lang-kotlin):
 * the Kotlin language server, Kotlin runs, ktlint and detekt, and the Kotlin debugger.
 * Browser mode answers from `mock/kotlinMock.ts` (the demo Gradle project, `?project=kotlin`).
 */
import { invoke, listen } from "./index";
import type { RunConfiguration } from "./run";
import type { InstallEvent } from "./java";

/** A `main` function (`kotlinforge_lang_kotlin::MainFunction`). */
export interface MainFunction {
  line: number;
  /** JVM class to run (`com.acme.orders.MainKt`). */
  class: string;
  /** Configuration name (`MainKt`). */
  name: string;
}

export interface ToolInfo {
  /** `setting`, `path`, `managed` or `gradle` (the project's Gradle plugin). */
  source: string;
  path: string;
}

export interface KotlinStatus {
  /** `kotlin-lsp` or `kotlin-language-server`. */
  server: string;
  serverPath: string | null;
  serverVersion: string | null;
  pinnedVersion: string;
  downloadSize: number | null;
  ktlint: ToolInfo | null;
  detekt: ToolInfo | null;
  kotlinc: string | null;
  problems: string[];
  remote: boolean;
}

export type LintSeverity = "error" | "warning" | "weak" | "info";

/** A ktlint or detekt finding (`kotlinforge_lang_kotlin::lint::LintProblem`), 0-based. */
export interface LintProblem {
  linter: "ktlint" | "detekt";
  file: string;
  line: number;
  column: number;
  severity: LintSeverity;
  message: string;
  rule: string;
}

export interface KotlinDebugPlan {
  /** `kotlin` (kotlin-lsp's debugger) or `java` (java-debug). */
  adapter: "kotlin" | "java";
  dapPort: number;
  hostName: string;
  jdwpPort: number;
  run: boolean;
}

export interface CoroutineFrame {
  method: string;
  file?: string;
  line?: number;
}

export interface Coroutine {
  name: string;
  id?: number;
  /** `RUNNING`, `SUSPENDED`, `CREATED`. */
  state: string;
  job: string;
  dispatcher?: string;
  frames: CoroutineFrame[];
  creation: CoroutineFrame[];
}

export type CoroutineDump = { kind: "coroutines"; coroutines: Coroutine[] } | { kind: "unavailable"; message: string };

export interface KotlinInstallProgress {
  id: "kotlin-lsp" | "ktlint" | "detekt";
  event: InstallEvent;
}

/** Kotlin: the full dump through `DebugProbes` (see `coroutines::DUMP_EXPRESSION`). */
export const COROUTINE_DUMP_EXPRESSION =
  "java.io.ByteArrayOutputStream().also { kotlinx.coroutines.debug.DebugProbes.dumpCoroutines(java.io.PrintStream(it)) }.toString()";
/** Java syntax: the coroutine infos (`coroutines::INFO_EXPRESSION`). */
export const COROUTINE_INFO_EXPRESSION = "kotlinx.coroutines.debug.internal.DebugProbesImpl.INSTANCE.dumpCoroutinesInfo().toString()";

export const kotlinApi = {
  status: () => invoke<KotlinStatus>("kotlin_status"),
  installLanguageServer: () => invoke<string>("kotlin_lsp_install"),
  installLinters: () => invoke<string[]>("kotlin_linters_install"),
  mainFunctions: (path: string, text: string) => invoke<MainFunction[]>("kotlin_main_functions", { path, text }),
  lint: (path: string, content: string) => invoke<LintProblem[]>("kotlin_lint", { path, content }),
  format: (path: string, content: string) => invoke<string | null>("kotlin_format", { path, content }),
  detekt: (paths?: string[]) => invoke<LintProblem[]>("kotlin_detekt", { paths: paths ?? null }),
  debugPrepare: (configuration: RunConfiguration) => invoke<KotlinDebugPlan>("kotlin_debug_prepare", { configuration }),
  coroutines: (value: string) => invoke<CoroutineDump>("kotlin_coroutines", { value }),
  onInstall: (cb: (p: KotlinInstallProgress) => void) => listen<KotlinInstallProgress>("kotlin://install", cb),
};
