/**
 * PyForge UI state: the project's interpreter (status bar, popup), its installed packages
 * (Python Packages tool window, inspections) and running package commands in the Run tool
 * window. Everything is computed in Rust (`src/ipc/python.ts`).
 */
import { createSignal } from "solid-js";
import { pythonApi, type CurrentInterpreter, type PackageAction, type PackageListing, type PythonInterpreter } from "@/ipc/python";
import type { CommandLine, RunConfiguration } from "@/ipc/run";
import { launch } from "@/features/run/runStore";
import { showBalloon } from "@/keymap";
import { reloadFile } from "@/state/editor";
import { isPluginActive } from "@/state/project";
import { getSetting, saveSettings } from "@/state/settings";
import { t as tr } from "@/i18n";

export const pythonActive = () => isPluginActive("forge.python-env");

const [current, setCurrent] = createSignal<CurrentInterpreter | null>(null);
const [listing, setListing] = createSignal<PackageListing | null>(null);
const [listError, setListError] = createSignal<string | null>(null);
const [loading, setLoading] = createSignal(false);
const [running, setRunning] = createSignal(0);
/** Bumped whenever the environment's packages changed: inspections run again. */
const [revision, setRevision] = createSignal(0);

export { current as currentInterpreter, listing as packageListing, listError, loading as packagesLoading, running as packagesRunning, revision as packagesRevision };

/** Read the interpreter in effect and list its packages. */
export async function refreshInterpreter(): Promise<void> {
  const cur = await pythonApi.current().catch(() => null);
  setCurrent(cur);
  await refreshPackages();
}

export async function refreshPackages(): Promise<void> {
  if (!current()) {
    setListing(null);
    setListError(null);
    return;
  }
  setLoading(true);
  try {
    setListing(await pythonApi.packages());
    setListError(null);
  } catch (e) {
    setListing(null);
    setListError(String(e));
  } finally {
    setLoading(false);
    setRevision((n) => n + 1);
  }
}

/** Select the project's interpreter: a local python path or a configured interpreter's name. */
export async function selectInterpreter(value: string): Promise<void> {
  await saveSettings({ "python.interpreter": value });
  await refreshInterpreter();
}

/** Configured interpreters of both settings layers. */
export function configuredInterpreters(): PythonInterpreter[] {
  const read = (key: string, project: boolean) =>
    (getSetting<Record<string, string>[]>(key, []) ?? []).map((t) => fromSetting(t, project)).filter((i) => i.name);
  return [...read("python.projectInterpreters", true), ...read("python.interpreters", false)];
}

/** The `objects` setting table of an interpreter (string values; lists one per line). */
export function toSetting(i: PythonInterpreter): Record<string, string> {
  const out: Record<string, string> = {};
  const put = (k: string, v: string) => {
    if (v.trim()) out[k] = v;
  };
  put("id", i.id);
  put("name", i.name);
  put("kind", i.kind);
  put("path", i.path);
  put("envDir", i.envDir);
  put("version", i.version);
  put("dockerServer", i.dockerServer);
  put("image", i.image);
  put("composeFiles", i.composeFiles.join("\n"));
  put("service", i.service);
  if (i.kind === "compose" && i.lifecycle === "exec") put("lifecycle", "exec");
  put("sshConfig", i.sshConfig);
  put("distribution", i.distribution);
  put("mappings", i.mappings.filter((m) => m.remote.trim()).map((m) => `${m.local}\t${m.remote}`).join("\n"));
  return out;
}

export function fromSetting(t: Record<string, string>, project: boolean): PythonInterpreter {
  const s = (k: string) => t[k] ?? "";
  return {
    ...emptyInterpreter(),
    id: s("id"),
    name: s("name"),
    kind: (s("kind") || "system") as PythonInterpreter["kind"],
    path: s("path"),
    envDir: s("envDir"),
    version: s("version"),
    dockerServer: s("dockerServer"),
    image: s("image"),
    composeFiles: s("composeFiles").split("\n").map((l) => l.trim()).filter(Boolean),
    service: s("service"),
    lifecycle: s("lifecycle") === "exec" ? "exec" : "run",
    sshConfig: s("sshConfig"),
    distribution: s("distribution"),
    mappings: s("mappings")
      .split("\n")
      .filter((l) => l.includes("\t"))
      .map((l) => ({ local: l.split("\t")[0], remote: l.split("\t")[1] })),
    project,
  };
}

export function emptyInterpreter(): PythonInterpreter {
  return {
    id: "",
    name: "",
    kind: "system",
    path: "",
    envDir: "",
    version: "",
    dockerServer: "",
    image: "",
    composeFiles: [],
    service: "",
    lifecycle: "run",
    sshConfig: "",
    distribution: "",
    mappings: [],
    project: false,
  };
}

export const KIND_LABELS: Record<PythonInterpreter["kind"], string> = {
  system: "System",
  pyenv: "pyenv",
  venv: "Virtualenv",
  virtualenv: "Virtualenv",
  uv: "uv",
  poetry: "Poetry",
  conda: "Conda",
  pipenv: "Pipenv",
  hatch: "Hatch",
  docker: "Docker",
  compose: "Docker Compose",
  ssh: "SSH",
  wsl: "WSL",
};

/** `Python 3.12 (shop-api)` for a detected environment. */
export function labelOf(i: { version: string; envDir: string; kind: PythonInterpreter["kind"]; name?: string; path?: string }): string {
  const v = i.version.split(".").slice(0, 2).join(".");
  const base = v ? `Python ${v}` : "Python";
  if (i.kind === "system" || i.kind === "pyenv") return base;
  if (["docker", "compose", "ssh", "wsl"].includes(i.kind)) return `${base} (${i.name})`;
  const parts = i.envDir.replace(/\\/g, "/").replace(/\/$/, "").split("/");
  const last = parts.pop() ?? "";
  const name = [".venv", "venv", "env", ".env"].includes(last) ? (parts.pop() ?? last) : last;
  return name ? `${base} (${name})` : base;
}

// ---- Running package commands -------------------------------------------------------------

/** Run a command line from Rust in the Run tool window; resolves with its exit code. */
export async function runCommandLine(name: string, cmd: CommandLine): Promise<number | null> {
  const cfg: RunConfiguration = { name, type: "python-command", options: { commandLine: JSON.stringify(cmd) } };
  const handle = await launch(cfg);
  return handle ? await handle.exited : null;
}

/** `pip install requests` for `/p/.venv/bin/python -m pip install requests`. */
export function commandName(cmd: CommandLine): string {
  const a = cmd.args;
  const pip = a.indexOf("pip");
  const program = cmd.program.replace(/\\/g, "/").split("/").pop() ?? cmd.program;
  const words =
    pip > 0 && a[pip - 1] === "-m"
      ? a.slice(pip)
      : [program, ...a.filter((x, i) => !(a[i - 1] === "--python") && x !== "--python")];
  const line = words.join(" ");
  return line.length > 60 ? `${line.slice(0, 57)}…` : line;
}

/** Install, uninstall or upgrade packages (requirement specs) with the environment's manager. */
export async function packageAction(action: PackageAction, packages: string[]): Promise<boolean> {
  if (!packages.length) return false;
  setRunning((n) => n + 1);
  try {
    const cmd = await pythonApi.packagesCommand(action, packages);
    const code = await runCommandLine(commandName(cmd), cmd);
    if (code !== 0) {
      if (code !== null) showBalloon(tr("langs.python.pythonPackagesFailed", { action }), tr("langs.python.exitCodeSeeTheRun", { code }), 5000, undefined, { type: "error" });
      return false;
    }
    await Promise.all(["pyproject.toml", "poetry.lock", "uv.lock", "Pipfile"].map((p) => reloadFile(p).catch(() => undefined)));
    return true;
  } catch (e) {
    showBalloon(tr("langs.python.pythonPackages"), String(e), 5000, undefined, { type: "error" });
    return false;
  } finally {
    setRunning((n) => n - 1);
    await refreshPackages();
  }
}
