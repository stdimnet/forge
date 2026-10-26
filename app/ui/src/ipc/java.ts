/**
 * JavaForge IPC (app/src-tauri/src/commands/java.rs; crates packs/javaforge/*): JDKs,
 * Eclipse JDT LS, Maven and Gradle builds and the Java debugger. Browser mode answers from
 * `mock/javaMock.ts` (the demo Gradle project, `?project=java`).
 */
import { invoke, listen } from "./index";
import type { RunConfiguration } from "./run";
import type { SchemaCompletion } from "./jsonSchema";

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
  /** The wrapper's Gradle version (`gradle-wrapper.properties`). */
  gradleVersion?: string;
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

/** A dependency of a resolved tree (`javaforge_build::model::DependencyNode`). */
export interface DependencyNode {
  /** `com.google.guava:guava:33.3.1-jre`, `project :core`. */
  id: string;
  group: string;
  artifact: string;
  version: string;
  /** The requested version when conflict resolution picked another. */
  requested?: string;
  /** Maven scope. */
  scope?: string;
  /** Already listed above (`(*)`). */
  omitted: boolean;
  failed: boolean;
  children: DependencyNode[];
}

export interface DependencyScope {
  /** `compileClasspath`, Maven: `Dependencies`. */
  name: string;
  description: string;
  dependencies: DependencyNode[];
}

export interface MavenPlugin {
  id: string;
  /** `compiler`. */
  prefix: string;
  version?: string;
  goals: string[];
}

export interface DeclaredDependency {
  configuration: string;
  group: string;
  artifact: string;
  version: string;
  line: number;
}

/** A project of the build (root, Gradle subproject, Maven module). */
export interface ProjectModel {
  name: string;
  /** Gradle path (`:`, `:app`); Maven: the module folder. */
  path: string;
  dir: string;
  tasks: Goal[];
  dependencies: DependencyScope[];
  plugins: MavenPlugin[];
  declared: DeclaredDependency[];
}

export interface MavenProfile {
  id: string;
  activeByDefault: boolean;
}

/** The Gradle / Maven tool window's model (`javaforge_build::model::BuildModel`). */
export interface BuildModel {
  tool: BuildToolId;
  projects: ProjectModel[];
  profiles: MavenProfile[];
  gradleVersion?: string;
  /** The data comes from a sync, not only from the build files. */
  synced: boolean;
  /** Unix seconds of the sync. */
  syncedAt?: number;
}

export interface GradleWrapper {
  version: string;
  distributionUrl: string;
  /** `bin` or `all`. */
  distributionType: string;
  sha256?: string;
}

export interface WrapperInfo {
  wrapper: GradleWrapper;
  path: string;
}

/** `distributionSha256Sum` against the checksum Gradle publishes. */
export type WrapperChecksum = "notSet" | "matches" | "mismatch" | "unknown";

/** "A newer version is available" (`javaforge_build::deps::OutdatedMarker`). */
export interface OutdatedDependency {
  group: string;
  artifact: string;
  current: string;
  latest: string;
  /** UTF-16 range of the version text. */
  from: number;
  to: number;
  line: number;
  start: { line: number; character: number };
  end: { line: number; character: number };
  message: string;
}

export interface SyncLine {
  line: string;
  stderr: boolean;
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
  buildModel: () => invoke<BuildModel | null>("java_build_model"),
  syncProject: () => invoke<BuildModel>("java_sync_project"),
  syncCancel: () => invoke<void>("java_sync_cancel"),
  onSync: (cb: (l: SyncLine) => void) => listen<SyncLine>("java://sync", cb),
  wrapperInfo: () => invoke<WrapperInfo | null>("java_wrapper_info"),
  wrapperVerify: () => invoke<WrapperChecksum>("java_wrapper_verify"),
  wrapperUpdate: (version?: string) => invoke<GradleWrapper>("java_wrapper_update", { version: version || null }),
  buildComplete: (path: string, text: string, offset: number) => invoke<SchemaCompletion | null>("java_build_complete", { path, text, offset }),
  buildOutdated: (path: string, text: string) => invoke<OutdatedDependency[]>("java_build_outdated", { path, text }),
  buildWorkspace: (full = false) => invoke<number>("java_build_workspace", { full }),
};
