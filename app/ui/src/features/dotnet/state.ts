/**
 * DotNetForge state in the UI: whether the .NET plugin is active, the host and SDK status
 * (`dotnet_status`), the solution and its projects (`dotnet_workspace`), the last build's
 * report (problems in the Build tool window and the editor), and running dotnet command
 * lines in the Run tool window through the hidden `dotnet-command` configuration type.
 */
import { createSignal } from "solid-js";
import { dotnetApi, type BuildReport, type DotnetAction, type DotnetProject, type DotnetStatus, type DotnetWorkspace } from "@/ipc/dotnet";
import type { CommandLine, RunConfiguration } from "@/ipc/run";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { launch } from "@/features/run/runStore";
import { publishDiagnostics } from "@/features/lsp/store";
import { balloonsFor } from "@/keymap/dispatch";
import { isPluginActive } from "@/state/project";

export const PLUGIN = "forge.dotnet";
export const dotnetActive = () => isPluginActive(PLUGIN);
export const showDotnetBalloon = balloonsFor(".NET");
export const errorText = (e: unknown) => String(e).replace(/^Error: /, "");

const [status, setStatus] = createSignal<DotnetStatus | null>(null);
export { status as dotnetStatus };

export async function refreshDotnetStatus(refresh = false): Promise<DotnetStatus | null> {
  try {
    const s = await dotnetApi.status(refresh);
    setStatus(s);
    return s;
  } catch {
    return null;
  }
}

const [workspace, setWorkspace] = createSignal<DotnetWorkspace | null>(null);
const [workspaceError, setWorkspaceError] = createSignal<string | null>(null);
const [loading, setLoading] = createSignal(false);
export { workspace as dotnetWorkspace, workspaceError as dotnetWorkspaceError, loading as dotnetLoading };

/** Read the solution again (`refresh`: from the files, not the cache). */
export async function refreshSolution(refresh = false): Promise<DotnetWorkspace | null> {
  setLoading(true);
  try {
    const ws = await dotnetApi.workspace(refresh);
    setWorkspace(ws);
    setWorkspaceError(null);
    return ws;
  } catch (e) {
    setWorkspaceError(errorText(e));
    return null;
  } finally {
    setLoading(false);
  }
}

export const runnableProjects = (): DotnetProject[] => (workspace()?.projects ?? []).filter((p) => !p.isTestProject && (p.web || /exe$/i.test(p.outputType)));
export const testProjects = (): DotnetProject[] => (workspace()?.projects ?? []).filter((p) => p.isTestProject);
export const projectByPath = (path: string) => workspace()?.projects.find((p) => p.path === path || p.name === path);

/** `dotnet build Bookstore.sln` for `/usr/share/dotnet/dotnet build Bookstore.sln -c Debug …`. */
export function commandName(cmd: CommandLine): string {
  const program = cmd.program.replace(/\\/g, "/").split("/").pop()?.replace(/\.exe$/, "") ?? cmd.program;
  const args = cmd.args.filter((a, i, all) => !["-nologo", "-v", "-c"].includes(a) && !["-v", "-c"].includes(all[i - 1] ?? ""));
  const line = [program, ...args].join(" ");
  return line.length > 64 ? `${line.slice(0, 61)}…` : line;
}

/** Run a command line in the Run tool window; resolves with its exit code. */
export async function runDotnetCommand(cmd: CommandLine, name = commandName(cmd), build = false): Promise<number | null> {
  const cfg: RunConfiguration = { name, type: "dotnet-command", options: { commandLine: JSON.stringify(cmd), build: String(build) } };
  const handle = await launch(cfg);
  return handle ? await handle.exited : null;
}

/** Output collected for `runDotnetCapture`, by capture id (read by the `dotnet-command` type). */
const captures = new Map<string, { chunks: string[]; done: (code: number | null) => void }>();
let captureSeq = 0;
export const captureFor = (id: string) => captures.get(id);

/** Run a command line in the Run tool window and collect its output (EF Core lists). */
export async function runDotnetCapture(cmd: CommandLine, name = commandName(cmd)): Promise<{ code: number | null; output: string }> {
  const id = `capture-${++captureSeq}`;
  let done!: (code: number | null) => void;
  const exited = new Promise<number | null>((r) => (done = r));
  const entry = { chunks: [] as string[], done };
  captures.set(id, entry);
  try {
    const cfg: RunConfiguration = { name, type: "dotnet-command", options: { commandLine: JSON.stringify(cmd), build: "false", capture: id } };
    const handle = await launch(cfg);
    if (!handle) return { code: null, output: "" };
    const code = await Promise.race([exited, handle.exited.then((c) => new Promise<number | null>((r) => setTimeout(() => r(c), 300)))]);
    return { code, output: entry.chunks.join("") };
  } finally {
    captures.delete(id);
  }
}

// ---- Builds ---------------------------------------------------------------------------------------

const [report, setReport] = createSignal<BuildReport | null>(null);
const [building, setBuilding] = createSignal<string | null>(null);
const [buildOutput, setBuildOutput] = createSignal("");
export { report as buildReport, building as dotnetBuilding, buildOutput as dotnetBuildOutput };
export const setDotnetBuilding = setBuilding;

/** Files that got build diagnostics, so the next build clears them. */
const published = new Set<string>();

const inProject = (path?: string) => !!path && !path.startsWith("/") && !/^[A-Za-z]:/.test(path);

/** Show a finished build's report: the Build tool window and editor diagnostics (`dotnet-build`). */
export function setBuildReport(r: BuildReport, output: string): void {
  setReport(r);
  setBuildOutput(output);
  const byFile = new Map<string, EditorDiagnostic[]>();
  for (const p of r.problems) {
    if (!inProject(p.path) || !/\.(cs|fs|vb|razor|cshtml)$/i.test(p.path ?? "")) continue;
    const d: EditorDiagnostic = {
      range: {
        start: { line: p.line, character: p.column },
        end: { line: p.endLine, character: p.endColumn > p.column || p.endLine > p.line ? p.endColumn : p.column + 1 },
      },
      severity: p.severity,
      message: p.message,
      source: "dotnet-build",
      code: p.code || null,
    };
    byFile.set(p.path!, [...(byFile.get(p.path!) ?? []), d]);
  }
  for (const f of published) if (!byFile.has(f)) publishDiagnostics(f, [], "dotnet-build");
  published.clear();
  for (const [file, diags] of byFile) {
    publishDiagnostics(file, diags, "dotnet-build");
    published.add(file);
  }
}

/** Parse a build's output (Rust) and show its problems. */
export async function publishBuildOutput(output: string): Promise<BuildReport | null> {
  try {
    const r = await dotnetApi.buildProblems(output);
    setBuildReport(r, output);
    return r;
  } catch {
    return null;
  }
}

const LABELS: Record<string, string> = { build: "Build", rebuild: "Rebuild", clean: "Clean", restore: "Restore", test: "Test", pack: "Pack", publish: "Publish", watch: "Watch" };

/** Build, Rebuild, Clean or Restore the solution or a project; resolves with the exit code. */
export async function runBuild(action: DotnetAction, target?: string): Promise<number | null> {
  try {
    const cmd = await dotnetApi.command(action, target);
    const what = target ? (projectByPath(target)?.name ?? target.split("/").pop()) : (workspace()?.solution?.split("/").pop() ?? "Solution");
    setBuilding(`${LABELS[action] ?? action} ${what}`);
    const code = await runDotnetCommand(cmd, `${LABELS[action] ?? action} ${what}`, ["build", "rebuild", "clean", "restore"].includes(action));
    return code;
  } catch (e) {
    showDotnetBalloon(`Cannot ${action}`, errorText(e), 8000);
    return null;
  } finally {
    setBuilding(null);
  }
}
