/**
 * PHP CLI interpreters (Rust: `packs/phpforge/interpreters`, `commands/php_interpreters.rs`).
 *
 * Interpreters are stored in two `objects` settings: `php.interpreters` (user settings, every
 * project) and `php.projectInterpreters` (project settings, "Visible only for this project").
 * `php.interpreter` selects the project's interpreter by name; a value that names none is the
 * path of a local php executable. Table values are strings: compose files one per line, path
 * mappings `local<TAB>remote` per line, configuration options `name=value` per line and
 * extensions comma separated (the same format the Rust model reads).
 */
import { invoke } from "@/ipc";
import type { SettingValue } from "@/ipc/settings";

export type InterpreterKind = "local" | "docker" | "compose" | "ssh" | "wsl";

export interface PathMapping {
  /** Local path: absolute, or relative to the project root (empty = the root). */
  local: string;
  /** Absolute path where PHP runs. */
  remote: string;
}

export interface ConfigOption {
  name: string;
  value: string;
}

export interface PhpInterpreter {
  id: string;
  name: string;
  kind: InterpreterKind;
  /** PHP executable: a local path, or the path in the container / on the host. */
  path: string;
  dockerServer: string;
  image: string;
  composeFiles: string[];
  service: string;
  lifecycle: "run" | "exec";
  /** Id of an SSH configuration (Settings | Tools | SSH Configurations). */
  sshConfig: string;
  distribution: string;
  mappings: PathMapping[];
  options: ConfigOption[];
  debuggerExtension: string;
  /** Detected on Reload. */
  version: string;
  configFile: string;
  additionalIni: string;
  xdebug: string;
  extensions: string[];
  /** In `php.projectInterpreters`. */
  project: boolean;
}

export interface InterpreterInfo {
  version: string;
  configFile: string;
  additionalIni: string;
  xdebug: string;
  extensions: string[];
}

export interface DetectedInterpreter {
  path: string;
  version: string;
}

export const GLOBAL_KEY = "php.interpreters";
export const PROJECT_KEY = "php.projectInterpreters";
export const SELECTED_KEY = "php.interpreter";

export const KIND_LABELS: Record<InterpreterKind, string> = {
  local: "Local",
  docker: "Docker",
  compose: "Docker Compose",
  ssh: "SSH",
  wsl: "WSL",
};

const lines = (s: string | undefined) =>
  (s ?? "")
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);

export function emptyInterpreter(kind: InterpreterKind = "local"): PhpInterpreter {
  return {
    id: "",
    name: "",
    kind,
    path: "",
    dockerServer: "",
    image: "",
    composeFiles: [],
    service: "",
    lifecycle: "run",
    sshConfig: "",
    distribution: "",
    mappings: [],
    options: [],
    debuggerExtension: "",
    version: "",
    configFile: "",
    additionalIni: "",
    xdebug: "",
    extensions: [],
    project: false,
  };
}

/** One table of an `objects` setting. */
export function decodeInterpreter(t: Record<string, string>, project: boolean): PhpInterpreter {
  const kind = (["docker", "compose", "ssh", "wsl"].includes(t.kind) ? t.kind : "local") as InterpreterKind;
  return {
    ...emptyInterpreter(kind),
    id: t.id ?? "",
    name: t.name ?? "",
    path: t.path ?? "",
    dockerServer: t.dockerServer ?? "",
    image: t.image ?? "",
    composeFiles: lines(t.composeFiles),
    service: t.service ?? "",
    lifecycle: t.lifecycle === "exec" ? "exec" : "run",
    sshConfig: t.sshConfig ?? "",
    distribution: t.distribution ?? "",
    mappings: (t.mappings ?? "")
      .split("\n")
      .filter((l) => l.includes("\t"))
      .map((l) => {
        const [local, remote] = l.split("\t");
        return { local: local.trim(), remote: remote.trim() };
      }),
    options: lines(t.options).map((l) => {
      const i = l.indexOf("=");
      return i < 0 ? { name: l, value: "" } : { name: l.slice(0, i).trim(), value: l.slice(i + 1).trim() };
    }),
    debuggerExtension: t.debuggerExtension ?? "",
    version: t.version ?? "",
    configFile: t.configFile ?? "",
    additionalIni: t.additionalIni ?? "",
    xdebug: t.xdebug ?? "",
    extensions: (t.extensions ?? "")
      .split(",")
      .map((e) => e.trim())
      .filter(Boolean),
    project,
  };
}

/** The table stored in the settings; empty fields are left out. */
export function encodeInterpreter(i: PhpInterpreter): Record<string, string> {
  const out: Record<string, string> = {};
  const put = (k: string, v: string) => {
    if (v.trim()) out[k] = v;
  };
  put("id", i.id);
  put("name", i.name.trim());
  put("kind", i.kind);
  put("path", i.path.trim());
  put("dockerServer", i.dockerServer.trim());
  put("image", i.image.trim());
  put("composeFiles", i.composeFiles.map((f) => f.trim()).filter(Boolean).join("\n"));
  put("service", i.service.trim());
  if (i.kind === "compose") put("lifecycle", i.lifecycle);
  put("sshConfig", i.sshConfig);
  put("distribution", i.distribution.trim());
  put(
    "mappings",
    i.mappings
      .filter((m) => m.remote.trim())
      .map((m) => `${m.local.trim()}\t${m.remote.trim()}`)
      .join("\n"),
  );
  put(
    "options",
    i.options
      .filter((o) => o.name.trim())
      .map((o) => `${o.name.trim()}=${o.value.trim()}`)
      .join("\n"),
  );
  put("debuggerExtension", i.debuggerExtension.trim());
  put("version", i.version);
  put("configFile", i.configFile);
  put("additionalIni", i.additionalIni);
  put("xdebug", i.xdebug);
  put("extensions", i.extensions.join(","));
  return out;
}

/** The interpreters of both settings values; project ones first (their names win). */
export function decodeInterpreters(global: SettingValue | undefined, project: SettingValue | undefined): PhpInterpreter[] {
  const read = (v: SettingValue | undefined, isProject: boolean) =>
    (Array.isArray(v) ? v : [])
      .filter((t): t is Record<string, string> => !!t && typeof t === "object" && !Array.isArray(t))
      .map((t) => decodeInterpreter(t, isProject))
      .filter((i) => i.name.trim());
  return [...read(project, true), ...read(global, false)];
}

/** The settings values of a list (split by `project`). */
export function encodeInterpreters(list: PhpInterpreter[]): { global: Record<string, string>[]; project: Record<string, string>[] } {
  return {
    global: list.filter((i) => !i.project).map(encodeInterpreter),
    project: list.filter((i) => i.project).map(encodeInterpreter),
  };
}

/** The interpreter `selected` names (by name or id), if any. */
export const findInterpreter = (list: PhpInterpreter[], selected: string) =>
  list.find((i) => i.name === selected.trim()) ?? list.find((i) => i.id && i.id === selected.trim());

/** `PHP 8.3.12 (Docker php:8.3-cli)`-style detail shown next to a name. */
export function interpreterDetail(i: PhpInterpreter): string {
  switch (i.kind) {
    case "docker":
      return `Docker ${i.image || "(no image)"}`;
    case "compose":
      return `Docker Compose ${i.service || "(no service)"}`;
    case "ssh":
      return `SSH ${i.path || "php"}`;
    case "wsl":
      return `WSL${i.distribution ? ` ${i.distribution}` : ""}`;
    default:
      return i.path || "php";
  }
}

/** `major.minor` → the language level it supports (clamped like Rust's `level_for_version`). */
export function levelForVersion(version: string, levels: readonly string[]): string | null {
  const m = /^(\d+)\.(\d+)/.exec(version.trim());
  if (!m) return null;
  const v = Number(m[1]) * 100 + Number(m[2]);
  const num = (l: string) => {
    const [a, b] = l.split(".").map(Number);
    return a * 100 + b;
  };
  return [...levels].reverse().find((l) => num(l) <= v) ?? levels[0] ?? null;
}

/** A unique id for a new interpreter. */
export function newInterpreterId(): string {
  return `php-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`;
}

/** A name not used by `list` yet (`PHP 8.3`, `PHP 8.3 (2)`). */
export function uniqueName(base: string, list: PhpInterpreter[], except?: PhpInterpreter): string {
  const taken = (n: string) => list.some((i) => i !== except && i.name === n);
  if (!taken(base)) return base;
  for (let n = 2; ; n++) if (!taken(`${base} (${n})`)) return `${base} (${n})`;
}

export const interpretersApi = {
  detect: () => invoke<DetectedInterpreter[]>("php_interpreters_detect"),
  info: (interpreter: PhpInterpreter) => invoke<InterpreterInfo>("php_interpreter_info", { interpreter }),
  phpinfo: (interpreter: PhpInterpreter) => invoke<string>("php_interpreter_phpinfo", { interpreter }),
  automaticMappings: (interpreter: PhpInterpreter) => invoke<PathMapping[]>("php_interpreter_mappings", { interpreter }),
  composeServices: (files: string[]) => invoke<string[]>("php_interpreter_compose_services", { files }),
  wslDistributions: () => invoke<string[]>("php_interpreter_wsl_distributions"),
};
