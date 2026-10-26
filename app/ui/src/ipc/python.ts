/**
 * PyForge IPC (app/src-tauri/src/commands/python.rs, crates packs/pyforge/*): Python
 * interpreters, the Python Packages tool window, requirements inspections and completion,
 * Reformat Code with ruff format / Black and the "Python" run configuration. Offsets are
 * UTF-16, lines 0-based.
 */
import { invoke } from "./index";
import type { CommandLine } from "./run";

export type EnvKind =
  | "system"
  | "pyenv"
  | "venv"
  | "virtualenv"
  | "uv"
  | "poetry"
  | "conda"
  | "pipenv"
  | "hatch"
  | "docker"
  | "compose"
  | "ssh"
  | "wsl";

export interface PathMapping {
  local: string;
  remote: string;
}

/** A configured or detected interpreter (`pyforge_interpreters::PythonInterpreter`). */
export interface PythonInterpreter {
  id: string;
  name: string;
  kind: EnvKind;
  /** The python executable (in the container / on the host for remote kinds). */
  path: string;
  envDir: string;
  version: string;
  dockerServer: string;
  image: string;
  composeFiles: string[];
  service: string;
  lifecycle: "run" | "exec";
  sshConfig: string;
  distribution: string;
  mappings: PathMapping[];
  project: boolean;
}

export interface DetectedPython {
  path: string;
  kind: EnvKind;
  version: string;
  envDir: string;
  /** Belongs to the open project. */
  project: boolean;
}

export interface CurrentInterpreter {
  interpreter: PythonInterpreter;
  /** `Python 3.12 (shop-api)`. */
  label: string;
  /** Chosen automatically (the project's environment). */
  automatic: boolean;
}

export interface InterpreterInfo {
  version: string;
  implementation: string;
  prefix: string;
  basePrefix: string;
  executable: string;
  platform: string;
}

export interface NewEnvironment {
  kind: EnvKind;
  location: string;
  baseInterpreter: string;
  pythonVersion: string;
  installDependencies: boolean;
}

export interface EnvironmentPlan {
  steps: CommandLine[];
  python: string;
}

export interface InstalledPackage {
  name: string;
  version: string;
  modules: string[];
}

export type PackageManager = "pip" | "uv" | "poetry" | "pipenv";

export interface PackageListing {
  interpreter: string;
  manager: PackageManager;
  packages: InstalledPackage[];
}

export type PackageAction = "install" | "uninstall" | "upgrade";

export interface PypiPackage {
  name: string;
  version: string;
  summary: string;
  license: string;
  requiresPython: string;
  author: string;
  homePage: string;
  versions: string[];
}

export interface SearchHit {
  name: string;
  version: string;
  summary: string;
}

export type PythonFix =
  | { kind: "installRequirements"; title: string; requirements: string[] }
  | { kind: "addRequirement"; title: string; requirement: string; target: string }
  | { kind: "ignore"; title: string; package: string };

export interface PythonProblem {
  line: number;
  start: number;
  end: number;
  severity: "error" | "warning" | "weak";
  message: string;
  code: string;
  fixes: PythonFix[];
}

export interface PythonCompletion {
  from: number;
  items: { label: string; detail: string; kind: "package" | "version" }[];
}

export interface Declared {
  name: string;
  normalized: string;
  specifier: string;
  written: string;
  marker?: string;
  direct: boolean;
  source: "requirements" | "pep621" | "poetry" | "dependencyGroup";
  group?: string;
  line: number;
  start: number;
  end: number;
}

export interface DepFile {
  deps: Declared[];
  errors: { line: number; start: number; end: number; message: string }[];
  includes: string[];
  projectName?: string;
  requiresPython?: string;
  hasPep621: boolean;
  hasPoetry: boolean;
}

export const pythonApi = {
  current: () => invoke<CurrentInterpreter | null>("python_interpreter_current"),
  detect: () => invoke<DetectedPython[]>("python_interpreters_detect"),
  info: (interpreter: PythonInterpreter) => invoke<InterpreterInfo>("python_interpreter_info", { interpreter }),
  environmentPlan: (request: NewEnvironment) => invoke<EnvironmentPlan>("python_environment_plan", { request }),
  packages: (interpreter?: string) => invoke<PackageListing | null>("python_packages_list", { interpreter: interpreter ?? null }),
  packagesCommand: (action: PackageAction, packages: string[], interpreter?: string) =>
    invoke<CommandLine>("python_packages_command", { action, packages, interpreter: interpreter ?? null }),
  latest: (names: string[]) => invoke<Record<string, string>>("python_packages_latest", { names }),
  search: (query: string) => invoke<SearchHit[]>("python_packages_search", { query }),
  packageInfo: (name: string) => invoke<PypiPackage>("python_package_info", { name }),
  inspect: (path: string, content: string) => invoke<PythonProblem[]>("python_inspect", { path, content }),
  addRequirement: (requirement: string, target: string, current?: string) =>
    invoke<{ path: string; text: string }>("python_add_requirement", { requirement, target, current: current ?? null }),
  complete: (path: string, content: string, offset: number) =>
    invoke<PythonCompletion | null>("python_complete", { path, content, offset }),
  runMarkers: (content: string) => invoke<{ line: number }[]>("python_run_markers", { content }),
  format: (path: string, content: string) => invoke<string | null>("python_format", { path, content }),
  dependencies: (path: string, content: string) => invoke<DepFile>("python_dependencies", { request: { path, content } }),
};
