/**
 * Browser-mode PyForge: the `python_*` commands for the FastAPI demo (`?project=python`).
 * Inspections, completion contexts, dependency files, "Add requirement", run markers, new
 * environments and package commands run the Rust crates through WebAssembly
 * (`crates/forge-wasm/src/python.rs`); the interpreters, installed packages and PyPI are
 * canned. Package commands change the in-memory environment only.
 */
import { emitMock, registerMock } from "@/ipc";
import type {
  CurrentInterpreter,
  DepFile,
  DetectedPython,
  EnvKind,
  InstalledPackage,
  PackageAction,
  PythonInterpreter,
  PythonProblem,
} from "@/ipc/python";
import { callWasm, forgeWasm } from "@/wasm";
import { isPythonDemo, pythonDemoInfo, pythonDemoInstalled, pythonDemoName, pythonDemoPypi } from "./demoPythonProject";
import { onMockDidOpen, setMockServerStatus } from "./lspMock";
import { mockSettingValue, setSetting } from "./settingsMock";
import { mockFiles } from "./workspaceMock";

const ROOT = `~/Projects/${pythonDemoName}`;
const call = async <T>(fn: string, input: unknown): Promise<T> => {
  const wasm = await forgeWasm();
  return callWasm<T>((wasm as unknown as Record<string, (json: string) => string>)[fn], input);
};

const DETECTED: DetectedPython[] = [
  { path: `${ROOT}/.venv/bin/python`, kind: "uv", version: "3.12.7", envDir: `${ROOT}/.venv`, project: true },
  { path: "/usr/bin/python3.13", kind: "system", version: "3.13.0", envDir: "", project: false },
  { path: "/usr/bin/python3.12", kind: "system", version: "3.12.3", envDir: "", project: false },
  { path: "~/miniconda3/envs/ml/bin/python", kind: "conda", version: "3.11.9", envDir: "~/miniconda3/envs/ml", project: false },
  { path: "~/.pyenv/versions/3.10.15/bin/python", kind: "pyenv", version: "3.10.15", envDir: "", project: false },
];

const installed: InstalledPackage[] = structuredClone(pythonDemoInstalled);

function blank(): PythonInterpreter {
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

function configured(): PythonInterpreter[] {
  const read = (key: string, project: boolean) =>
    ((mockSettingValue(key) as Record<string, string>[] | undefined) ?? []).map((t) => ({
      ...blank(),
      ...t,
      kind: (t.kind || "system") as EnvKind,
      composeFiles: (t.composeFiles ?? "").split("\n").filter(Boolean),
      lifecycle: t.lifecycle === "exec" ? ("exec" as const) : ("run" as const),
      mappings: [],
      project,
    }));
  return [...read("python.projectInterpreters", true), ...read("python.interpreters", false)];
}

async function current(): Promise<CurrentInterpreter | null> {
  if (!isPythonDemo()) return null;
  const selected = String(mockSettingValue("python.interpreter") ?? "");
  const conf = configured().find((i) => i.name === selected);
  const detected = DETECTED.find((d) => d.path === (selected || DETECTED[0].path));
  const interpreter: PythonInterpreter | undefined =
    conf ?? (detected ? { ...blank(), name: detected.path, kind: detected.kind, path: detected.path, envDir: detected.envDir, version: detected.version } : selected ? { ...blank(), name: selected, path: selected } : undefined);
  if (!interpreter) return null;
  const label = await call<string>("python_interpreter_label", interpreter);
  return { interpreter, label, automatic: !selected };
}

if (isPythonDemo()) {
  registerMock("project_detect", () => structuredClone(pythonDemoInfo));
  setSetting("project.announcedFrameworks", [`${ROOT}#python`, `${ROOT}#${pythonDemoInfo.primary?.id}`], "user");
}

registerMock("python_interpreter_current", () => current());
registerMock("python_interpreters_detect", () => (isPythonDemo() ? structuredClone(DETECTED) : []));
registerMock("python_interpreter_info", ({ interpreter }) => {
  const i = interpreter as PythonInterpreter;
  return {
    version: i.kind === "docker" ? "3.12.7" : i.version || "3.12.7",
    implementation: "CPython",
    prefix: i.envDir || "/usr",
    basePrefix: "/usr",
    executable: i.path || "python3",
    platform: "linux",
  };
});

registerMock("python_environment_plan", async ({ request }) => {
  const req = request as { kind: EnvKind; location: string };
  const location = req.location.startsWith("/") || req.location.startsWith("~") ? req.location : `${ROOT}/${req.location}`;
  const steps = await call<{ program: string; args: string[] }[]>("python_environment_plan", {
    request: { ...req, location },
    files: { pyproject: true, uvLock: true, requirements: ["requirements.txt"] },
  });
  return {
    steps: steps.map((s) => ({ ...s, cwd: ROOT })),
    python: req.kind === "poetry" ? "" : `${location.replace(/\/$/, "")}/bin/python`,
  };
});

registerMock("python_packages_list", async () => {
  const cur = await current();
  if (!cur) return null;
  return { interpreter: cur.label, manager: cur.interpreter.kind === "uv" ? "uv" : "pip", packages: structuredClone(installed) };
});

const norm = (n: string) => n.toLowerCase().replace(/[-_.]+/g, "-");

registerMock("python_packages_command", async ({ action, packages }) => {
  const cur = await current();
  if (!cur) throw new Error("No Python interpreter is configured for the project");
  const list = packages as string[];
  // The in-memory environment changes right away; the Run tool window shows the command.
  for (const spec of list) {
    const name = spec.split(/[<>=!~;[ ]/)[0];
    const i = installed.findIndex((p) => norm(p.name) === norm(name));
    if (action === "uninstall") {
      if (i >= 0) installed.splice(i, 1);
      continue;
    }
    const pinned = /==\s*([\w.]+)/.exec(spec)?.[1];
    const known = pythonDemoPypi.find((p) => norm(p.name) === norm(name));
    const version = pinned ?? known?.version ?? "1.0.0";
    if (i >= 0) installed[i] = { ...installed[i], version };
    else installed.push({ name: known?.name ?? name, version, modules: [name.replace(/-/g, "_").toLowerCase()] });
  }
  installed.sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
  const answer = await call<{ program: string; args: string[] }>("python_packages_command", {
    kind: cur.interpreter.kind,
    uvLock: true,
    action: action as PackageAction,
    packages: list,
    python: cur.interpreter.path,
  });
  return { program: answer.program, args: answer.args, cwd: ROOT, env: { PIP_DISABLE_PIP_VERSION_CHECK: "1" } };
});

registerMock("python_packages_latest", ({ names }) =>
  Object.fromEntries(
    (names as string[]).flatMap((n) => {
      const p = pythonDemoPypi.find((x) => norm(x.name) === norm(n));
      return p ? [[n, p.version]] : [];
    }),
  ),
);

registerMock("python_packages_search", ({ query }) => {
  const q = norm(String(query));
  return pythonDemoPypi
    .filter((p) => norm(p.name).includes(q) || p.summary.toLowerCase().includes(String(query).toLowerCase()))
    .map(({ name, version, summary }) => ({ name, version, summary }));
});

registerMock("python_package_info", ({ name }) => {
  const p = pythonDemoPypi.find((x) => norm(x.name) === norm(String(name)));
  if (!p) throw new Error("not found on PyPI");
  const [major, minor] = p.version.split(".").map(Number);
  const versions = [p.version, `${major}.${minor}.0`, `${major}.${Math.max(minor - 1, 0)}.2`, `${major}.${Math.max(minor - 1, 0)}.0`].filter((v, i, a) => a.indexOf(v) === i);
  return { ...p, author: "", versions };
});

// ---- Inspections and completion (Rust through WebAssembly) ---------------------------------

const depFiles = () => Object.keys(mockFiles).filter((p) => p === "pyproject.toml" || /^(requirements|constraints)[\w.-]*\.(txt|in)$/.test(p));

async function importContext() {
  const declared: string[] = [];
  let target: string | undefined;
  let projectName: string | undefined;
  for (const path of depFiles()) {
    const f = await call<DepFile>("python_dependencies", { path, content: mockFiles[path] });
    declared.push(...f.deps.map((d) => d.normalized));
    if (path === "pyproject.toml") {
      projectName = f.projectName;
      if (f.hasPep621 || f.hasPoetry) target = path;
    }
  }
  target ??= depFiles().find((p) => p !== "pyproject.toml");
  const local = new Set<string>();
  for (const path of Object.keys(mockFiles)) {
    const rel = path.startsWith("src/") ? path.slice(4) : path;
    const top = rel.split("/")[0];
    if (rel.includes("/")) local.add(top);
    else if (top.endsWith(".py")) local.add(top.slice(0, -3));
  }
  const ignored = (mockSettingValue("python.inspections.ignoredPackages") as string[] | undefined) ?? [];
  return { declared, installed, localModules: [...local], projectName, target, ignored };
}

registerMock("python_inspect", async ({ path, content }) => {
  if (!isPythonDemo()) return [];
  const imports = await importContext();
  if (String(path).endsWith(".py") && mockSettingValue("python.inspections.imports") === false) return [];
  const check = mockSettingValue("python.inspections.requirements") !== false;
  return call<PythonProblem[]>("python_inspect", { path, content, installed: check ? installed : null, imports });
});

registerMock("python_add_requirement", async ({ requirement, target, current }) => {
  const text = (current as string | null) ?? mockFiles[String(target)] ?? "";
  const next = await call<string>("python_add_requirement", { target, text, requirement });
  return { path: target, text: next };
});

registerMock("python_complete", async ({ path, content, offset }) => {
  type Ctx = { kind: "name"; from: number; prefix: string } | { kind: "version"; from: number; prefix: string; package: string };
  const ctx = await call<Ctx | null>("python_completion_context", { path, content, offset });
  if (!ctx) return null;
  if (ctx.kind === "name") {
    const q = ctx.prefix.toLowerCase();
    const items = [
      ...installed.filter((p) => p.name.toLowerCase().startsWith(q)).map((p) => ({ label: p.name, detail: `${p.version} (installed)`, kind: "package" })),
      ...pythonDemoPypi
        .filter((p) => p.name.toLowerCase().startsWith(q) && !installed.some((i) => norm(i.name) === norm(p.name)))
        .map((p) => ({ label: p.name, detail: p.summary, kind: "package" })),
    ];
    return { from: ctx.from, items };
  }
  const p = pythonDemoPypi.find((x) => norm(x.name) === norm(ctx.package));
  const have = installed.find((x) => norm(x.name) === norm(ctx.package))?.version;
  const versions = [...new Set([p?.version, have].filter(Boolean) as string[])];
  return {
    from: ctx.from,
    items: versions.filter((v) => v.startsWith(ctx.prefix)).map((v) => ({ label: v, detail: v === have ? "installed" : "", kind: "version" })),
  };
});

registerMock("python_run_markers", ({ content }) => call("python_run_markers", { content }));
registerMock("python_dependencies", ({ request }) => call("python_dependencies", request));
// ruff and Black are local processes; browser mode formats nothing.
registerMock("python_format", ({ content }) => content);

// ---- basedpyright for the demo's Python files -----------------------------------------------

/** What basedpyright reports for the demo files (rule names as in its output). */
function pyrightDiagnostics(text: string) {
  const out: { range: { start: { line: number; character: number }; end: { line: number; character: number } }; severity: string; message: string; source: string; code: string }[] = [];
  text.split("\n").forEach((line, i) => {
    const attr = line.indexOf("order.totl");
    if (attr >= 0)
      out.push({
        range: { start: { line: i, character: attr + 6 }, end: { line: i, character: attr + 10 } },
        severity: "error",
        message: 'Cannot access attribute "totl" for class "Order"\n  Attribute "totl" is unknown',
        source: "basedpyright",
        code: "reportAttributeAccessIssue",
      });
    const rich = line.indexOf("rich.console");
    if (rich >= 0 && line.startsWith("from"))
      out.push({
        range: { start: { line: i, character: rich }, end: { line: i, character: rich + 12 } },
        severity: "warning",
        message: 'Import "rich.console" could not be resolved',
        source: "basedpyright",
        code: "reportMissingImports",
      });
  });
  return out;
}

if (isPythonDemo()) {
  onMockDidOpen((path) => {
    if (!path.endsWith(".py")) return;
    setMockServerStatus({ id: "basedpyright", name: "basedpyright", state: "ready" });
    setTimeout(() => emitMock("lsp://diagnostics", { path, diagnostics: pyrightDiagnostics(mockFiles[path] ?? "") }), 250);
  });
}
