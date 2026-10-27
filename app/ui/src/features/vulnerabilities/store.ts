/**
 * The vulnerable dependency check's state: the last results per audit tool, the
 * "Vulnerable Dependencies" problems published for the manifests (diagnostics source
 * `vulnerabilities`, inspection VulnerableDependencies), the Upgrade quick fix and the
 * install step after it. Detection, parsing, problems and edits are computed in Rust
 * (`forge-vulns`); this module renders and forwards.
 */
import { createSignal } from "solid-js";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { workspaceApi } from "@/ipc/workspace";
import {
  TOOL_LABELS,
  vulnerabilitiesApi,
  type CheckResult,
  type TargetResult,
  type ToolId,
  type Upgrade,
  type Vulnerability,
  type VulnerabilityProblem,
} from "@/ipc/vulnerabilities";
import type { RunConfiguration } from "@/ipc/run";
import { viewFor } from "@/editor/views";
import { balloonsFor } from "@/keymap";
import { publishDiagnostics } from "@/features/lsp/store";
import { replaceChangedPart } from "@/features/lsp/format";
import { setProblemsTab } from "@/features/lsp/ProblemsView";
import { launch } from "@/features/run/runStore";
import { safeMode } from "@/features/trust/store";
import { reloadFile } from "@/state/editor";
import { showToolWindow } from "@/state/layout";

export const VULNERABILITIES_TAB = "vulnerabilities";
export const SOURCE = "vulnerabilities";
export const COMMAND_TYPE = "vulnerabilities-command";

export const showBalloon = balloonsFor("Package Checker");

const [results, setResults] = createSignal<TargetResult[]>([]);
const [enabled, setEnabled] = createSignal(true);
const [running, setRunning] = createSignal(false);
const [error, setError] = createSignal<string | null>(null);
/** Problems last published per file (for Alt+Enter). */
const [problems, setProblems] = createSignal<Record<string, VulnerabilityProblem[]>>({});
export { results as vulnerabilityResults, running as vulnerabilitiesRunning, error as vulnerabilitiesError, enabled as vulnerabilitiesEnabled, problems as vulnerabilityProblems };

export const fileName = (path: string) => path.split("/").pop() ?? path;

/** The result whose manifests include `path`. */
export const resultFor = (path: string): TargetResult | undefined => results().find((r) => r.annotate.includes(path));

export const vulnerabilityCount = () => results().reduce((n, r) => n + r.vulnerabilities.length, 0);
export const packageCount = () => new Set(results().flatMap((r) => r.vulnerabilities.map((v) => `${r.tool}:${v.package}`))).size;

const toDiagnostic = (p: VulnerabilityProblem): EditorDiagnostic => ({
  range: { start: { line: p.line, character: p.start }, end: { line: p.line, character: p.end } },
  severity: p.severity,
  message: p.message,
  source: "Vulnerable Dependencies",
  code: p.ids?.[0] ?? null,
});

let published = new Set<string>();

/** Compute and publish the problems of one manifest (its editor text when it is open). */
export async function publishFile(path: string, text?: string): Promise<VulnerabilityProblem[]> {
  const r = resultFor(path);
  if (!r || (!r.vulnerabilities.length && !r.notices?.length)) {
    if (published.has(path)) publishDiagnostics(path, [], SOURCE);
    setProblems((prev) => ({ ...prev, [path]: [] }));
    return [];
  }
  const content = text ?? viewFor(path)?.state.doc.toString() ?? (await workspaceApi.readFile(path).catch(() => null));
  if (content == null) return [];
  const list = await vulnerabilitiesApi.problems(path, content, r.vulnerabilities, r.notices ?? []).catch(() => [] as VulnerabilityProblem[]);
  setProblems((prev) => ({ ...prev, [path]: list }));
  publishDiagnostics(path, list.map(toDiagnostic), SOURCE);
  published.add(path);
  return list;
}

async function publishAll(): Promise<void> {
  const paths = new Set(results().flatMap((r) => r.annotate));
  for (const old of published) if (!paths.has(old)) publishDiagnostics(old, [], SOURCE);
  published = new Set([...published].filter((p) => paths.has(p)));
  await Promise.all([...paths].map((p) => publishFile(p)));
}

/** Forget everything (project closed, check turned off). */
export function clearResults(): void {
  for (const p of published) publishDiagnostics(p, [], SOURCE);
  published = new Set();
  setResults([]);
  setProblems({});
  setError(null);
}

export interface CheckOptions {
  /** Run the tools even when the cached result is still valid. */
  force?: boolean;
  /** Only these tools (Composer | Audit). */
  tools?: ToolId[];
  /** Open the Vulnerable Dependencies tab. */
  reveal?: boolean;
  /** Automatic checks: a balloon only when something was found. */
  quiet?: boolean;
}

/** Check the project's dependencies (Code | Analyze Code | Check for Vulnerable Dependencies). */
export async function runCheck(opts: CheckOptions = {}): Promise<CheckResult | null> {
  if (running()) return null;
  if (safeMode()) {
    if (!opts.quiet) showBalloon("Dependencies are not checked in Safe Mode", "The audit tools run the project's tooling. Trust the project to check them.", 6000, undefined, { type: "warning" });
    return null;
  }
  setRunning(true);
  setError(null);
  if (opts.reveal) {
    setProblemsTab(VULNERABILITIES_TAB);
    showToolWindow("problems");
  }
  try {
    const check = await vulnerabilitiesApi.check(!!opts.force, opts.tools);
    setEnabled(check.enabled);
    if (!check.enabled) {
      clearResults();
      if (!opts.quiet) showBalloon("Vulnerable dependency check is turned off", "Turn it on in Settings | Tools | Package Checker.", 6000);
      return check;
    }
    const before = new Set(results().flatMap(keys));
    setResults((prev) => (opts.tools ? [...prev.filter((r) => !opts.tools!.includes(r.tool)), ...check.results] : check.results));
    await publishAll();
    // Automatic checks speak up only about vulnerabilities that were not reported before.
    const fresh = check.results.flatMap(keys).some((k) => !before.has(k));
    if (!opts.quiet || fresh) report(check.results, !!opts.quiet);
    return check;
  } catch (e) {
    const message = String(e instanceof Error ? e.message : e).replace(/^Error: /, "");
    setError(message);
    if (!opts.quiet) showBalloon("Checking for vulnerable dependencies failed", message, 8000, undefined, { type: "error" });
    return null;
  } finally {
    setRunning(false);
  }
}

const keys = (r: TargetResult) => r.vulnerabilities.map((v) => `${r.tool}:${v.package}:${v.installed ?? ""}:${v.id}`);

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

function report(list: TargetResult[], quiet: boolean): void {
  const vulns = list.flatMap((r) => r.vulnerabilities);
  const packages = new Set(list.flatMap((r) => r.vulnerabilities.map((v) => `${r.tool}:${v.package}`))).size;
  const reveal = { label: "Show Vulnerable Dependencies", run: () => (setProblemsTab(VULNERABILITIES_TAB), showToolWindow("problems")) };
  if (vulns.length) {
    showBalloon(
      `Found ${plural(vulns.length, "vulnerability", "vulnerabilities")} in ${plural(packages, "package")}`,
      list.filter((r) => r.vulnerabilities.length).map((r) => TOOL_LABELS[r.tool]).join(", "),
      8000,
      [reveal],
      { type: "warning" },
    );
    return;
  }
  if (quiet) return;
  const missing = list.filter((r) => r.status === "missing" || r.status === "failed");
  if (missing.length) {
    showBalloon("Some dependencies could not be checked", missing.map((r) => r.message ?? TOOL_LABELS[r.tool]).join("\n"), 8000, [reveal], { type: "warning" });
  } else if (!list.length) {
    showBalloon("No dependencies to check", "Forge found no composer.json, package.json, requirements file, pyproject.toml, Cargo.toml or go.mod in the project root.");
  } else {
    showBalloon("No vulnerable dependencies found", list.map((r) => TOOL_LABELS[r.tool]).join(", "));
  }
}

/** Run a command line from the backend in the Run tool window; resolves to its exit code. */
async function runCommandLine(name: string, commandLine: unknown): Promise<number | null> {
  const cfg: RunConfiguration = { name, type: COMMAND_TYPE, options: { commandLine: JSON.stringify(commandLine) } };
  const handle = await launch(cfg);
  return handle ? await handle.exited : null;
}

const installLabel = (cmd: { program: string; args: string[] }) => {
  const program = fileName(cmd.program.replace(/\\/g, "/"));
  const pip = cmd.args.indexOf("pip");
  const args = pip > 0 && cmd.args[pip - 1] === "-m" ? cmd.args.slice(pip) : [program, ...cmd.args];
  return args.join(" ");
};

/** Install what the manifest requires now (`composer update <pkg>`, `npm install`, ...). */
export async function runInstall(r: TargetResult, upgrade?: Pick<Upgrade, "package" | "fixed">): Promise<void> {
  try {
    const cmd = await vulnerabilitiesApi.installCommand(r.tool, r.dir, upgrade?.package, upgrade?.fixed);
    const code = await runCommandLine(installLabel(cmd), cmd);
    if (code !== 0) {
      if (code !== null) showBalloon(`${installLabel(cmd)} failed`, `Exit code ${code}. See the Run tool window.`, 6000, undefined, { type: "error" });
      return;
    }
    await Promise.all(r.inputs.map((p) => reloadFile(p).catch(() => undefined)));
    await runCheck({ quiet: true });
  } catch (e) {
    showBalloon("Cannot install", String(e).replace(/^Error: /, ""), 6000, undefined, { type: "error" });
  }
}

/** Install the audit tool itself (pip-audit into the project interpreter), after asking. */
export async function installTool(r: TargetResult): Promise<void> {
  try {
    const cmd = await vulnerabilitiesApi.installCommand(r.tool, r.dir);
    const code = await runCommandLine(installLabel(cmd), cmd);
    if (code === 0) await runCheck({ force: true, tools: [r.tool], reveal: true });
  } catch (e) {
    showBalloon(`Cannot install ${TOOL_LABELS[r.tool]}`, String(e).replace(/^Error: /, ""), 6000, undefined, { type: "error" });
  }
}

/** "Upgrade to <fixed>": edit the constraint (in the editor when the file is open), then
 * offer to install. */
export async function applyUpgrade(path: string, upgrade: Upgrade): Promise<void> {
  const r = resultFor(path);
  const view = viewFor(path);
  try {
    const text = view?.state.doc.toString() ?? (await workspaceApi.readFile(path));
    const next = await vulnerabilitiesApi.upgrade(path, text, upgrade.package, upgrade.fixed);
    if (next == null) {
      showBalloon(`Cannot upgrade ${upgrade.package}`, `${fileName(path)} does not declare it with a version constraint.`, 5000, undefined, { type: "warning" });
      return;
    }
    if (view) replaceChangedPart(view, next, "input");
    else await workspaceApi.writeFile(path, next);
    const install = r ? [{ label: installTitle(r, upgrade), run: () => void runInstall(r, upgrade) }] : undefined;
    showBalloon(`Upgraded ${upgrade.package} to ${upgrade.constraint} in ${fileName(path)}`, r ? "Install the new version to fix the vulnerability." : undefined, 10000, install);
  } catch (e) {
    showBalloon(`Cannot upgrade ${upgrade.package}`, String(e).replace(/^Error: /, ""), 6000, undefined, { type: "error" });
  }
}

/** The install action's label for a tool. */
export function installTitle(r: TargetResult, upgrade?: Pick<Upgrade, "package">): string {
  switch (r.tool) {
    case "composer":
      return `Run composer update ${upgrade?.package ?? ""}`.trim();
    case "pip-audit":
      return upgrade ? `Install ${upgrade.package}` : "Install pip-audit";
    case "cargo-audit":
      return `Run cargo update -p ${upgrade?.package ?? ""}`.trim();
    case "govulncheck":
      return "Run go mod tidy";
    case "pnpm":
      return "Run pnpm install";
    case "yarn-classic":
    case "yarn-berry":
      return "Run yarn install";
    default:
      return "Run npm install";
  }
}

/** Where a package is declared itself (`direct`), or else where it is reported (on the
 * dependency that pulls it in, or in a lock file). */
export function declarationOf(r: TargetResult, v: Pick<Vulnerability, "package">, direct = true): { path: string; problem: VulnerabilityProblem } | null {
  for (const path of r.annotate) {
    const p = problems()[path]?.find((x) => x.package === v.package && x.dependency === v.package);
    if (p) return { path, problem: p };
  }
  if (direct) return null;
  for (const path of r.annotate) {
    const p = problems()[path]?.find((x) => x.package === v.package);
    if (p) return { path, problem: p };
  }
  return null;
}
