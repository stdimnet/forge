/**
 * Build problems, like CLion's Build tool window messages: the compiler and linker messages
 * of the last build (parsed in Rust, `cpp_build_problems`) in the Problems tool window's
 * "Build" tab, and as editor diagnostics of the files they name (source `cmake-build`).
 * Notes (candidates, declarations) are nested under their message.
 */
import { createSignal, For, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { Hammer } from "lucide-solid";
import { cppApi, type BuildProblem } from "@/ipc/cpp";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { severityIcon } from "@/features/lsp/severity";
import { publishDiagnostics } from "@/features/lsp/store";
import { openFileAt } from "@/editor/views";
import { showToolWindow } from "@/state/layout";
import { setProblemsTab } from "@/features/lsp/ProblemsView";

export const BUILD_TAB = "cpp-build";

const [problems, setProblems] = createSignal<BuildProblem[]>([]);
const [summary, setSummary] = createSignal("");
export { problems as buildProblems };

/** Files that got build diagnostics, so the next build clears them. */
const published = new Set<string>();

const sev = (p: BuildProblem): EditorDiagnostic["severity"] => (p.severity === "note" ? "info" : p.severity);
const inProject = (p: BuildProblem) => !!p.path && !p.path.startsWith("/") && !/^[A-Za-z]:/.test(p.path);

/** Show the problems of a finished build. */
export function setBuildProblems(list: BuildProblem[], exitCode: number | null): void {
  setProblems(list);
  const errors = list.filter((p) => p.severity === "error").length;
  const warnings = list.filter((p) => p.severity === "warning").length;
  setSummary(
    exitCode === 0
      ? `Build finished${warnings ? ` with ${warnings} warning${warnings === 1 ? "" : "s"}` : ""}`
      : `Build failed: ${errors} error${errors === 1 ? "" : "s"}, ${warnings} warning${warnings === 1 ? "" : "s"}`,
  );
  const byFile = new Map<string, EditorDiagnostic[]>();
  for (const p of list) {
    if (!inProject(p) || p.line === 0) continue;
    const line = p.line - 1;
    const col = Math.max(0, p.column - 1);
    const d: EditorDiagnostic = {
      range: { start: { line, character: col }, end: { line, character: col + 1 } },
      severity: sev(p),
      message: p.message,
      source: p.tool === "linker" ? "linker" : "compiler",
      code: p.code ?? null,
    };
    byFile.set(p.path!, [...(byFile.get(p.path!) ?? []), d]);
  }
  for (const f of published) if (!byFile.has(f)) publishDiagnostics(f, [], "cmake-build");
  published.clear();
  for (const [file, diags] of byFile) {
    publishDiagnostics(file, diags, "cmake-build");
    published.add(file);
  }
  if (errors) {
    setProblemsTab(BUILD_TAB);
    showToolWindow("problems");
  }
}

/** Parse a build's output (Rust) and show its problems. */
export async function publishBuildOutput(output: string, exitCode: number | null): Promise<void> {
  try {
    setBuildProblems(await cppApi.buildProblems(output), exitCode);
  } catch {
    /* the build output stays in the Run tool window */
  }
}

function reveal(p: BuildProblem): void {
  if (p.path && p.line) void openFileAt(p.path, p.line, p.column || 1);
  else if (p.path && inProject(p)) void openFileAt(p.path);
}

function Row(props: { p: BuildProblem; depth: number }) {
  const place = () => {
    const name = (props.p.path ?? props.p.file).replace(/\\/g, "/").split("/").pop();
    if (!name) return props.p.tool;
    return props.p.line ? `${name}:${props.p.line}${props.p.column ? `:${props.p.column}` : ""}` : name;
  };
  return (
    <>
      <div
        class="tree-row problem-row cpp-build-row"
        style={{ "padding-left": `${12 + props.depth * 20}px` }}
        data-testid="cpp-build-problem"
        tabIndex={0}
        onClick={() => reveal(props.p)}
        onKeyDown={(e) => e.key === "Enter" && reveal(props.p)}
        title={props.p.path ?? props.p.file}
      >
        <span class={`problem-icon sev-${sev(props.p)}`}>
          <Dynamic component={severityIcon[sev(props.p)]} size={14} />
        </span>
        <span class="problem-message">{props.p.message}</span>
        <Show when={props.p.code}>
          <span class="tree-hint">[{props.p.code}]</span>
        </Show>
        <span class="tree-hint">{place()}</span>
      </div>
      <For each={props.p.notes ?? []}>{(n) => <Row p={n} depth={props.depth + 1} />}</For>
    </>
  );
}

export function BuildProblemsView() {
  return (
    <div class="cpp-build-problems problems-tree">
      <Show when={summary()}>
        <div class="cpp-build-summary">
          <Hammer size={13} />
          <span>{summary()}</span>
        </div>
      </Show>
      <Show when={problems().length} fallback={<div class="problems-empty">{summary() ? "No compiler or linker messages" : "Build the project (Ctrl+F9) to see its compiler and linker messages"}</div>}>
        <For each={problems()}>{(p) => <Row p={p} depth={0} />}</For>
      </Show>
    </div>
  );
}
