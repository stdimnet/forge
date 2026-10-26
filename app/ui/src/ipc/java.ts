/**
 * JavaForge IPC (app/src-tauri/src/commands/java.rs; crates packs/javaforge/*): JDKs,
 * Eclipse JDT LS, Maven and Gradle builds and the Java debugger. Browser mode answers from
 * `mock/javaMock.ts` (the demo Gradle project, `?project=java`).
 */
import { invoke, listen } from "./index";
import type { RunConfiguration } from "./run";

/** A JDK on this machine (`javaforge_jdk::Jdk`). */
export interface Jdk {
  home: string;
  /** `21.0.4`; empty without a `release` file. */
  version: string;
  major: number;
  /** `Eclipse Temurin`, `Azul Zulu`, … */
  vendor: string;
  /** `JAVA_HOME`, `PATH`, `System`, `SDKMAN!`, `asdf`, `jabba`, `Gradle`, `IntelliJ IDEA`, `Forge`, `Windows Registry`, `Added`. */
  source: string;
  /** `temurin-21`. */
  name: string;
}

export interface AvailableReleases {
  available_lts_releases: number[];
  available_releases: number[];
  most_recent_lts: number;
  most_recent_feature_release: number;
}

export type BuildToolId = "maven" | "gradle";

export interface BuildModule {
  name: string;
  dir: string;
  path?: string;
}

export interface BuildProject {
  tool: BuildToolId;
  name: string;
  wrapper: boolean;
  buildFile: string;
  settingsFile?: string;
  kotlinDsl: boolean;
  modules: BuildModule[];
  jacoco: boolean;
  application: boolean;
  mainClass?: string;
  springBoot: boolean;
  customTasks: string[];
  javaVersion?: string;
  testFrameworks: ("jUnit5" | "jUnit4" | "testNg")[];
}

export interface Goal {
  name: string;
  /** `Lifecycle`, `Plugins`, `build`, `verification`, … */
  group: string;
  description: string;
}

export interface BuildView {
  project: BuildProject;
  goals: Goal[];
  /** `/home/me/inventory/gradlew`, `mvn`. */
  program: string;
}

export interface JavaStatus {
  jdtls?: string | null;
  jdtlsDir?: string | null;
  javaDebug?: string | null;
  serverJdk?: Jdk | null;
  projectJdk?: Jdk | null;
  languageLevel?: string | null;
  build?: BuildProject | null;
  /** Why JDT LS does not run; empty when it does. */
  problems: string[];
  remote: boolean;
}

export interface MainMethod {
  /** 0-based line of `main`. */
  line: number;
  /** Binary class name (`com.acme.App`). */
  class: string;
  name: string;
}

export interface DebugPlan {
  dapPort: number;
  hostName: string;
  jdwpPort: number;
  /** Start the configuration with `FORGE_JAVA_DEBUG_PORT` first (not for Remote JVM Debug). */
  run: boolean;
}

/** `java://install` progress of a JDT LS or JDK install (`forge_langservers::managed::Event`). */
export type InstallEvent =
  | { kind: "step"; message: string }
  | { kind: "progress"; label: string; done: number; total?: number | null }
  | { kind: "output"; stream: string; line: string };

export interface InstallProgress {
  id: "jdtls" | "jdk";
  event: InstallEvent;
}

/** The environment variable that turns a run into a debug run (see `launch::DEBUG_PORT_ENV`). */
export const DEBUG_PORT_ENV = "FORGE_JAVA_DEBUG_PORT";

export const javaApi = {
  status: () => invoke<JavaStatus>("java_status"),
  jdks: (refresh = false) => invoke<Jdk[]>("java_jdks", { refresh }),
  inspectJdk: (path: string) => invoke<Jdk | null>("java_jdk_inspect", { path }),
  releases: () => invoke<AvailableReleases>("java_jdk_releases"),
  downloadJdk: (feature: number) => invoke<Jdk>("java_jdk_download", { feature }),
  installLanguageServer: () => invoke<string>("java_jdtls_install"),
  buildInfo: () => invoke<BuildView | null>("java_build_info"),
  reloadProject: () => invoke<void>("java_reload_project"),
  mainMethods: (path: string, text: string) => invoke<MainMethod[]>("java_main_methods", { path, text }),
  debugPrepare: (configuration: RunConfiguration) => invoke<DebugPlan>("java_debug_prepare", { configuration }),
  onInstall: (cb: (p: InstallProgress) => void) => listen<InstallProgress>("java://install", cb),
};
