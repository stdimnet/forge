/**
 * DotNetForge (`app/src-tauri/src/commands/dotnet.rs`, logic in `packs/dotnetforge/*`): the
 * .NET host and SDKs, the solution and its projects, build command lines and the problems of
 * their output, `dotnet new` templates, NuGet search, versions and updates, Code Cleanup
 * with `dotnet format`, netcoredbg launch requests and the Roslyn / netcoredbg downloads.
 * Everything is computed in Rust.
 */
import type { LaunchRequest } from "@/features/debug/dapApi";
import { invoke, listen } from "./index";
import type { CommandLine, RunConfiguration } from "./run";

export interface DotnetHost {
  path: string;
  root: string;
  source: string;
}

export interface DotnetSdk {
  version: string;
  path: string;
}

export interface DotnetRuntime {
  name: string;
  version: string;
  path: string;
}

export interface InstallOption {
  label: string;
  command?: string;
  url?: string;
}

export interface ToolLocation {
  path: string;
  /** `setting`, `managed` or `path`. */
  source: string;
  version?: string;
}

export interface DotnetStatus {
  host?: DotnetHost | null;
  hosts: DotnetHost[];
  sdks: DotnetSdk[];
  runtimes: DotnetRuntime[];
  /** The SDK the project builds with (global.json). */
  sdk?: string | null;
  requestedSdk?: string | null;
  problems: string[];
  install: InstallOption[];
  roslyn?: ToolLocation | null;
  roslynProblem?: string | null;
  roslynVersion: string;
  roslynSize?: number | null;
  netcoredbg?: ToolLocation | null;
  netcoredbgProblem?: string | null;
  netcoredbgVersion: string;
  remote: boolean;
}

export type ProjectLanguage = "cSharp" | "fSharp" | "visualBasic" | "other";

export interface SolutionProject {
  name: string;
  path: string;
  language: ProjectLanguage;
  id?: string;
  folder?: string;
}

export interface Solution {
  path: string;
  name: string;
  format: "sln" | "slnx";
  projects: SolutionProject[];
  folders: string[];
  configurations: string[];
  problems?: string[];
}

export interface PackageReference {
  id: string;
  version: string;
  /** `project`, `central`, `override`, `props` or `implicit`. */
  source: string;
  privateAssets: boolean;
  condition?: string;
  line?: number | null;
}

export interface LaunchProfile {
  name: string;
  commandName: string;
  commandLineArgs?: string;
  workingDirectory?: string;
  applicationUrl?: string;
  launchBrowser: boolean;
  launchUrl?: string;
  environmentVariables?: Record<string, string>;
  line: number;
}

export type TestFramework = "xunit" | "nunit" | "mstest";

export interface DotnetProject {
  name: string;
  path: string;
  language: ProjectLanguage;
  sdk?: string | null;
  outputType: string;
  targetFrameworks: string[];
  assemblyName: string;
  rootNamespace: string;
  web: boolean;
  isTestProject: boolean;
  testFramework?: TestFramework | null;
  centralPackageManagement: boolean;
  packages: PackageReference[];
  projectReferences: string[];
  frameworkReferences?: string[];
  compileOrder?: string[];
  properties: Record<string, string>;
  launchProfiles?: LaunchProfile[];
  problems?: string[];
}

export interface GlobalJson {
  version?: string | null;
  rollForward: string;
  allowPrerelease: boolean;
  testRunner?: string | null;
}

export interface DotnetWorkspace {
  solutions: Solution[];
  solution?: string | null;
  projects: DotnetProject[];
  otherProjects?: string[];
  globalJson?: GlobalJson | null;
  configuration: string;
  problems?: string[];
}

export interface BuildProblem {
  severity: "error" | "warning" | "info";
  code: string;
  message: string;
  path?: string;
  /** 0-based. */
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  /** The output named a position (`line` 0, `column` 0 is then `:1:1`). */
  located?: boolean;
  project?: string;
  origin?: string;
}

export interface BuildReport {
  problems: BuildProblem[];
  outputs: { project: string; path: string }[];
  succeeded?: boolean | null;
  errors: number;
  warnings: number;
  elapsed?: string;
}

export interface Template {
  name: string;
  shortNames: string[];
  languages: string[];
  tags: string[];
  project: boolean;
}

export interface NewProjectRequest {
  template: string;
  name: string;
  directory?: string;
  language?: string;
  framework?: string;
  solution?: string;
}

export interface NuGetPackage {
  id: string;
  version: string;
  description: string;
  authors: string;
  totalDownloads: number;
  verified: boolean;
  iconUrl?: string | null;
  projectUrl?: string | null;
  licenseUrl?: string | null;
  tags?: string[];
  versions?: string[];
  vulnerable: boolean;
  deprecated: boolean;
}

export interface SearchResult {
  totalHits: number;
  packages: NuGetPackage[];
}

export interface PackageUpdate {
  id: string;
  installed: string;
  latest: string;
}

export interface FormatChange {
  path: string;
  line: number;
  column: number;
  diagnostic: string;
  description: string;
}

/** Build actions of the Build tool window and the Build menu. */
export type DotnetAction = "build" | "rebuild" | "clean" | "restore" | "test" | "pack" | "publish" | "watch";

/** `dotnet://install` while the Roslyn server or netcoredbg downloads. */
export interface InstallEvent {
  id: "roslyn" | "netcoredbg";
  event: { kind: "step"; message: string } | { kind: "progress"; label: string; done: number; total?: number | null } | { kind: "output"; line: string };
}

export const dotnetApi = {
  status: (refresh = false) => invoke<DotnetStatus>("dotnet_status", { refresh }),
  installRoslyn: () => invoke<string>("dotnet_roslyn_install"),
  installNetcoredbg: () => invoke<string>("dotnet_netcoredbg_install"),
  workspace: (refresh = false) => invoke<DotnetWorkspace>("dotnet_workspace", { refresh }),
  command: (action: DotnetAction, target?: string) => invoke<CommandLine>("dotnet_command", { action, target }),
  buildProblems: (output: string) => invoke<BuildReport>("dotnet_build_problems", { output }),
  templates: () => invoke<{ templates: Template[]; fromSdk: boolean; error?: string | null }>("dotnet_templates"),
  newProject: (request: NewProjectRequest) => invoke<CommandLine[]>("dotnet_new_project", { request }),
  nugetSearch: (query: string, prerelease?: boolean, skip = 0, take = 25) =>
    invoke<SearchResult>("nuget_search", { query, prerelease, skip, take }),
  nugetVersions: (id: string) => invoke<string[]>("nuget_versions", { id }),
  nugetUpdates: (prerelease?: boolean) => invoke<PackageUpdate[]>("nuget_updates", { prerelease }),
  nugetCommand: (action: "add" | "remove" | "update", project: string, pkg: string, version?: string, prerelease?: boolean) =>
    invoke<CommandLine>("nuget_command", { action, project, package: pkg, version, prerelease }),
  format: (path: string, content: string) => invoke<string | null>("dotnet_format", { path, content }),
  cleanupCommand: (paths?: string[]) => invoke<{ command: CommandLine; reportDir: string }>("dotnet_cleanup_command", { paths }),
  formatReport: (reportDir: string) => invoke<FormatChange[]>("dotnet_format_report", { reportDir }),
  debugRequest: (configuration: RunConfiguration) => invoke<LaunchRequest>("dotnet_debug_request", { configuration }),
  attachRequest: (pid: number, name?: string) => invoke<LaunchRequest>("dotnet_attach_request", { pid, name }),
  onInstall: (cb: (e: InstallEvent) => void) => listen<InstallEvent>("dotnet://install", cb),
  onWorkspace: (cb: (ws: DotnetWorkspace) => void) => listen<DotnetWorkspace>("dotnet://workspace", cb),
};
