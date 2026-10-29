/**
 * JavaForge Scala IPC (app/src-tauri/src/commands/scala.rs; crate packs/javaforge/lang-scala):
 * Metals, the sbt build and main objects. Browser mode answers from `mock/jvmExtrasMock.ts`
 * (the sbt demo, `?project=scala`).
 */
import { invoke, listen } from "./index";

export interface ScalaStatus {
  metals: string | null;
  metalsVersion: string | null;
  pinnedVersion: string;
  coursier: string | null;
  sbt: string | null;
  problems: string[];
  remote: boolean;
}

export interface SbtDependency {
  group: string;
  artifact: string;
  version: string;
  /** `test`, `runtime`, … (absent: compile). */
  configuration?: string;
  /** `%%`: cross-built for the Scala version. */
  cross: boolean;
}

export interface SbtProject {
  id: string;
  name: string;
  /** Folder relative to the build root (empty for the root). */
  dir: string;
  scalaVersion?: string;
  dependsOn: string[];
  aggregate: string[];
  dependencies: SbtDependency[];
  current: boolean;
}

export interface SbtBuild {
  sbtVersion?: string;
  scalaVersion?: string;
  organization?: string;
  projects: SbtProject[];
  plugins: SbtDependency[];
  synced: boolean;
  syncedAt?: number;
}

/** A `main` method, `App` object or Scala 3 `@main` (`javaforge_lang_scala::MainObject`). */
export interface MainObject {
  class: string;
  name: string;
  line: number;
}

export interface ToolLine {
  line: string;
  stderr: boolean;
}

/** The sbt tasks every project lists (`javaforge_lang_scala::sbt::TASKS`). */
export const SBT_TASKS: { name: string; description: string }[] = [
  { name: "compile", description: "Compiles sources." },
  { name: "test", description: "Executes all tests." },
  { name: "run", description: "Runs a main class, passing along arguments provided on the command line." },
  { name: "console", description: "Starts the Scala interpreter with the project classes on the classpath." },
  { name: "package", description: "Produces the main artifact, such as a binary jar." },
  { name: "clean", description: "Deletes files produced by the build, such as generated sources, compiled classes, and task caches." },
  { name: "update", description: "Resolves and optionally retrieves dependencies, producing a report." },
  { name: "doc", description: "Generates API documentation." },
  { name: "publishLocal", description: "Publishes artifacts to the local Ivy repository." },
  { name: "reload", description: "(Re)loads the current project or changes to another project." },
];

export const scalaApi = {
  status: () => invoke<ScalaStatus>("scala_status"),
  installMetals: () => invoke<string>("scala_metals_install"),
  mainObjects: (text: string) => invoke<MainObject[]>("scala_main_objects", { text }),
  sbtModel: () => invoke<SbtBuild | null>("scala_sbt_model"),
  sbtImport: () => invoke<SbtBuild>("scala_sbt_import"),
  sbtCancel: () => invoke<void>("scala_sbt_cancel"),
  onSbt: (cb: (l: ToolLine) => void) => listen<ToolLine>("scala://sbt", cb),
  onInstall: (cb: (l: ToolLine) => void) => listen<ToolLine>("scala://install", cb),
};
