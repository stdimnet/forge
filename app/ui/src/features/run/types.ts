/**
 * Run configuration types and debug launchers: the UI contribution points of the run
 * feature. Plugins call `registerRunConfigurationType` to add a type to the Run/Debug
 * Configurations dialog (with its form fields and, optionally, a command builder), and a
 * debugger plugin (e.g. Xdebug) calls `registerDebugLauncher` to handle Debug (Shift+F9).
 *
 * Types that a plugin manifest declares with `fields` and a `command` template work without
 * a UI registration; the backend expands the template.
 */
import { createMemo, createRoot, createSignal, type Component } from "solid-js";
import { Box, FileCode, FlaskConical, Hexagon, Moon, Package, Play, SquareTerminal, Terminal, TestTubeDiagonal } from "lucide-solid";
import type { CommandLine, DebugTarget, RunConfiguration, RunRequest, RunStarted } from "@/ipc/run";
import { settingsState } from "@/state/settings";
import type { TestFrameworkSpec } from "@/ipc/testing";

export type FieldKind = "string" | "path" | "text" | "enum" | "bool" | "segmented";

export interface RunConfigField {
  /** Key in `options`, or one of the common fields `arguments` / `interpreterOptions`. */
  key: string;
  label: string;
  kind?: FieldKind;
  /** Choices of `enum` and `segmented` fields; a function for choices that change (servers). */
  options?: { value: string; label: string }[] | (() => { value: string; label: string }[]);
  placeholder?: string;
  default?: string;
  required?: boolean;
  /** Shown only when this returns true (e.g. the "Method" field for scope "method"). */
  visible?: (cfg: RunConfiguration) => boolean;
  /** Browse for a directory instead of a file. */
  directory?: boolean;
}

export type IconComponent = Component<{ size?: number; class?: string; "stroke-width"?: number }>;

export interface RunConfigurationTypeDef {
  id: string;
  name: string;
  description?: string;
  icon: IconComponent;
  /** CSS color of the icon (a theme variable). */
  color?: string;
  /** Plugin that contributes the type; the type is hidden while it is disabled. */
  pluginId?: string;
  fields: RunConfigField[];
  /** Interpreter row: which interpreter the type uses, if any. */
  interpreter?: "php" | "node" | "shell" | "custom";
  /** The project default shown in the interpreter row (e.g. the `lua.interpreter` setting). */
  defaultInterpreter?: () => string;
  /** Label of the arguments row; `false` hides it. */
  argumentsLabel?: string | false;
  debuggable?: boolean;
  /** Produces a test tree (TeamCity, JUnit, TAP, … output, see `forge-output`). */
  testRunner?: boolean;
  /** Can run with coverage (Run with Coverage). */
  coverage?: boolean;
  /** No "Run on" choice (the type starts its runs itself or has its own form). */
  noRunTarget?: boolean;
  /** Initial values of a new configuration. */
  defaults?: () => Partial<RunConfiguration>;
  /** Name for a new or temporary configuration, derived from its fields. */
  suggestName?: (cfg: RunConfiguration) => string | undefined;
  /** Build the command in the UI; without it the backend's kind registry builds it. */
  buildCommand?: (cfg: RunConfiguration) => CommandLine;
  /**
   * Start runs through another backend command that streams the same `run://<runId>` events
   * (e.g. HTTP requests of scratches); without it `run_start` starts a process.
   */
  start?: (cfg: RunConfiguration, request: RunRequest) => Promise<RunStarted>;
  /** Not offered in the Run/Debug Configurations dialog (runs are started from the editor). */
  hidden?: boolean;
  /** Called once a run or debug run has started (e.g. open the served page in the browser). */
  afterStart?: (cfg: RunConfiguration, handle: RunHandle, debug: boolean) => void;
  /** Output filters (`$FILE_PATH$:$LINE$`) whose matches link files in the console. */
  outputFilters?: (cfg: RunConfiguration) => string[] | undefined;
  /** Extra check besides required fields; returns the problem. */
  validate?: (cfg: RunConfiguration) => string | null;
  /**
   * The type's own form instead of fields, interpreter, working directory and Before launch
   * (Compound). It changes the configuration through `update`; `configurations` are the ones
   * the dialog currently shows.
   */
  form?: Component<{
    cfg: RunConfiguration;
    update: (fn: (cfg: RunConfiguration) => void) => void;
    configurations: RunConfiguration[];
  }>;
}

/** Type id of compound configurations (their members start together). */
export const COMPOUND = "compound";

/** Member names of a compound (option `configurations`, one per line). */
export const compoundMembers = (cfg: RunConfiguration): string[] => [
  ...new Set(
    (cfg.options?.configurations ?? "")
      .split("\n")
      .map((n) => n.trim())
      .filter(Boolean),
  ),
];

const [registered, setRegistered] = createSignal<RunConfigurationTypeDef[]>([]);

/** Add (or replace, by id) a run configuration type. */
export function registerRunConfigurationType(def: RunConfigurationTypeDef): void {
  setRegistered((prev) => [...prev.filter((t) => t.id !== def.id), def]);
}

const ICONS: Record<string, IconComponent> = {
  "file-code": FileCode,
  "flask-conical": FlaskConical,
  package: Package,
  "square-terminal": SquareTerminal,
  terminal: Terminal,
  moon: Moon,
  hexagon: Hexagon,
  box: Box,
  "test-tube-diagonal": TestTubeDiagonal,
};

const SCOPE_LABELS: Record<string, string> = {
  all: "All tests",
  directory: "Directory",
  file: "File",
  test: "Test",
  pattern: "Pattern",
};

/** A run configuration type for a test framework a manifest declares (`testFrameworks`). */
export function testFrameworkType(spec: TestFrameworkSpec, pluginId: string): RunConfigurationTypeDef {
  const scopes = ["all", "directory", "file", "test", "pattern"].filter((s) => spec.scopes?.[s]);
  const scope = (cfg: RunConfiguration) => cfg.options?.scope || "all";
  const inScope = (...list: string[]) => (cfg: RunConfiguration) => list.includes(scope(cfg));
  return {
    id: spec.id,
    name: spec.name,
    description: spec.description,
    icon: ICONS[spec.icon ?? ""] ?? FlaskConical,
    color: "var(--success)",
    pluginId,
    argumentsLabel: false,
    debuggable: !!spec.debuggable,
    testRunner: true,
    coverage: !!spec.coverage,
    fields: [
      {
        key: "scope",
        label: "Test scope",
        kind: "segmented",
        default: "all",
        options: scopes.map((s) => ({ value: s, label: SCOPE_LABELS[s] })),
      },
      { key: "directory", label: "Directory", kind: "path", directory: true, required: true, visible: inScope("directory") },
      { key: "file", label: "File", kind: "path", required: true, visible: inScope("file", "test") },
      { key: "test", label: "Test", required: true, visible: inScope("test") },
      { key: "pattern", label: "Pattern", required: true, visible: inScope("pattern") },
      { key: "executable", label: "Executable", kind: "path", placeholder: spec.program },
      { key: "testRunnerOptions", label: "Test runner options" },
    ],
    suggestName: (cfg) => {
      const o = cfg.options ?? {};
      switch (scope(cfg)) {
        case "directory":
          return baseName(o.directory);
        case "file":
          return baseName(o.file)?.replace(/\.[^.]+$/, "");
        case "test":
          return o.test || undefined;
        case "pattern":
          return o.pattern ? `${spec.name}: ${o.pattern}` : undefined;
        default:
          return spec.name;
      }
    },
  };
}

/** Types from plugin manifests that declare a command template but have no UI registration. */
const manifestTypes = createRoot(() =>
  createMemo<RunConfigurationTypeDef[]>(() => {
    const out: RunConfigurationTypeDef[] = [];
    for (const p of settingsState.plugins) {
      for (const f of p.contributions?.testFrameworks ?? []) out.push(testFrameworkType(f, p.id));
      for (const t of p.contributions?.runConfigurationTypes ?? []) {
        if (!t.command?.length) continue;
        out.push({
          id: t.id,
          name: t.name,
          description: t.description,
          icon: ICONS[t.icon ?? ""] ?? Play,
          pluginId: p.id,
          debuggable: t.debuggable,
          interpreter: t.command.includes("${interpreter}") ? "custom" : undefined,
          argumentsLabel: t.command.includes("${arguments}") ? "Arguments" : false,
          fields: (t.fields ?? []).map((f) => ({
            key: f.key,
            label: f.label,
            kind: f.kind ?? "string",
            options: f.options,
            default: f.default,
            required: f.required,
          })),
          defaults: () => ({ interpreter: t.defaultInterpreter ?? "" }),
          suggestName: (cfg) => baseName(cfg.options?.file),
        });
      }
    }
    return out;
  }),
);

const enabled = (pluginId: string | undefined) =>
  !pluginId || !settingsState.plugins.length || settingsState.plugins.some((p) => p.id === pluginId && p.enabled);

/** All available types: registered ones win over manifest-only ones with the same id. */
export const runConfigurationTypes = createRoot(() =>
  createMemo(() => {
    const reg = registered();
    const ids = new Set(reg.map((t) => t.id));
    return [...reg, ...manifestTypes().filter((t) => !ids.has(t.id))]
      .filter((t) => enabled(t.pluginId) && !t.hidden)
      .sort((a, b) => a.name.localeCompare(b.name));
  }),
);

export const findType = (id: string): RunConfigurationTypeDef | undefined =>
  runConfigurationTypes().find((t) => t.id === id) ?? registered().find((t) => t.id === id);

export function baseName(path: string | undefined): string | undefined {
  if (!path) return undefined;
  const name = path.replace(/[\\/]+$/, "").split(/[\\/]/).pop();
  return name || undefined;
}

/** Values of a new configuration of `type`, with field defaults applied. */
export function newConfiguration(type: RunConfigurationTypeDef, name: string): RunConfiguration {
  const options: Record<string, string> = {};
  for (const f of type.fields) if (f.default !== undefined && !isCommonField(f.key)) options[f.key] = f.default;
  const base = type.defaults?.() ?? {};
  return {
    name,
    type: type.id,
    storeAsProjectFile: false,
    workingDirectory: "",
    env: {},
    interpreter: "",
    interpreterOptions: "",
    arguments: "",
    beforeLaunch: [],
    ...base,
    options: { ...options, ...(base.options ?? {}) },
  };
}

export const isCommonField = (key: string) => key === "arguments" || key === "interpreterOptions";

/** The choices of an `enum` / `segmented` field. */
export const fieldOptions = (f: RunConfigField): { value: string; label: string }[] =>
  typeof f.options === "function" ? f.options() : (f.options ?? []);

export function fieldValue(cfg: RunConfiguration, key: string): string {
  if (key === "arguments") return cfg.arguments ?? "";
  if (key === "interpreterOptions") return cfg.interpreterOptions ?? "";
  return cfg.options?.[key] ?? "";
}

/** Fields that are required, visible and empty: "File is not specified". */
export function validateConfiguration(cfg: RunConfiguration): string | null {
  const type = findType(cfg.type);
  if (!type) return `Unknown configuration type "${cfg.type}"`;
  if (!cfg.name.trim()) return "Name is empty";
  for (const f of type.fields) {
    if (!f.required || (f.visible && !f.visible(cfg))) continue;
    if (!fieldValue(cfg, f.key).trim()) return `${f.label} is not specified`;
  }
  return type.validate?.(cfg) ?? null;
}

// ---- Debug launchers ------------------------------------------------------------------

export interface RunHandle {
  started: RunStarted;
  /** Resolves with the exit code when the process ends. */
  exited: Promise<number | null>;
}

export interface DebugLaunchContext {
  /**
   * Start the configuration in the Run/Debug tool window with extra settings; `debugPort`
   * asks its run target to make the debugger's port reachable (published, forwarded or
   * reverse-tunnelled).
   */
  run: (extra?: Pick<RunRequest, "env" | "interpreterArgs" | "debugPort">) => Promise<RunHandle | null>;
  /**
   * The configuration's run target for the debugger ("Run on"): host names and path
   * mappings; `null` when it runs on this machine.
   */
  target: () => Promise<DebugTarget | null>;
}

export interface DebugLauncher {
  id: string;
  /** Whether this launcher can debug configurations of `type`. */
  canDebug: (cfg: RunConfiguration, type: RunConfigurationTypeDef | undefined) => boolean;
  launch: (cfg: RunConfiguration, ctx: DebugLaunchContext) => void | Promise<void>;
}

const [launchers, setLaunchers] = createSignal<DebugLauncher[]>([]);

/** Register a debugger for Debug (Shift+F9). Later registrations win. */
export function registerDebugLauncher(l: DebugLauncher): () => void {
  setLaunchers((prev) => [l, ...prev.filter((x) => x.id !== l.id)]);
  return () => setLaunchers((prev) => prev.filter((x) => x !== l));
}

export function debugLauncherFor(cfg: RunConfiguration): DebugLauncher | undefined {
  return launchers().find((l) => l.canDebug(cfg, findType(cfg.type)));
}
