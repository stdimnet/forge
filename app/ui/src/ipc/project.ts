/** Project type detection and the New Project wizard (Rust: `crates/forge-project`). */
import { invoke, listen } from "./index";

export interface Framework {
  /** `laravel`, `react`, `love`, ... or a language id when used as `primary`. */
  id: string;
  name: string;
  version?: string;
  language: string;
  /** Folder relative to the project root where it was found; missing for the root. */
  dir?: string;
}

export interface ProjectInfo {
  languages: string[];
  frameworks: Framework[];
  /** `phpforge`, `webforge`, `luaforge`, `pyforge`, `javaforge`, `goforge`, `rustforge`, `rubyforge`, `kotlinforge`, `cforge`, `dotnetforge`. */
  packs: string[];
  tools: string[];
  primary?: Framework;
}

export interface GeneratorChoice {
  value: string;
  label: string;
}

export type OptionValue = string | boolean;

/** `option` has one of `values`. */
export interface OptionCondition {
  option: string;
  values: string[];
}

export interface GeneratorOption {
  id: string;
  label: string;
  kind: "enum" | "bool" | "string";
  default: OptionValue;
  choices?: GeneratorChoice[];
  description?: string;
  /** Hint in an empty text field. */
  placeholder?: string;
  /** Shown (and used) only while every condition holds; hidden options take their default. */
  visibleWhen?: OptionCondition[];
  /** IPC command returning fresh `choices` (may use the network: only on a click). */
  refresh?: string;
}

export interface Generator {
  id: string;
  name: string;
  group: "PHP" | "Web" | "Java" | "Lua" | "Other" | string;
  pack: string;
  description: string;
  icon: string;
  defaultName: string;
  tools: string[];
  fallback?: string;
  options: GeneratorOption[];
}

export interface CreateArgs {
  generator: string;
  location: string;
  name: string;
  options: Record<string, OptionValue>;
}

export interface Plan {
  target: string;
  preview: string[];
  /** Files a built-in template writes, relative to the project. */
  files?: string[];
  missingTools: string[];
  fallback?: string;
  error?: string;
}

export type CreateEvent =
  | { type: "step"; index: number; total: number; label: string }
  | { type: "output"; line: string; stderr: boolean }
  | { type: "done"; root: string; name: string }
  | { type: "failed"; error: string };

export interface Detected {
  root: string;
  info: ProjectInfo;
}

export const DETECTED_EVENT = "project://detected";
export const CREATE_EVENT = "project://create";

export const projectApi = {
  detect: () => invoke<ProjectInfo>("project_detect"),
  generators: () => invoke<Generator[]>("project_generators_list"),
  plan: (args: CreateArgs) => invoke<Plan>("project_plan", { ...args }),
  /** Resolves with the project root; progress arrives as `project://create` events. */
  create: (args: CreateArgs) => invoke<string>("project_create", { ...args }),
  cancel: () => invoke<void>("project_create_cancel"),
  /** Fresh choices of an option with `refresh` (its IPC command). */
  /** Fresh choices of an option with a `refresh` command, for `generator`. */
  refreshChoices: (command: string, generator: string) => invoke<GeneratorChoice[]>(command, { generator }),
  onDetected: (cb: (d: Detected) => void) => listen<Detected>(DETECTED_EVENT, cb),
  onCreate: (cb: (e: CreateEvent) => void) => listen<CreateEvent>(CREATE_EVENT, cb),
};
