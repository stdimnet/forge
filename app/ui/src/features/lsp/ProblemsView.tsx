/**
 * Problems tool window (Alt+6): "Current File" and "Project Errors" tabs with a
 * file -> problems tree. Clicking a problem opens the file at that position.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { setToolWindowTab, toolWindowTab } from "@/state/layout";
import { Dynamic } from "solid-js/web";
import { ChevronDown, ChevronRight } from "lucide-solid";
import { activeTab } from "@/state/editor";
import { FileIcon } from "@/shell/FileIcon";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { diagnostics, filesWithProblems, sortDiagnostics } from "./store";
import { revealLocation } from "./navigation";
import { pluralize, severityIcon } from "./severity";
import { baseName } from "./util";
import { problemsTabs } from "./problemsTabs";
import { t } from "@/i18n";

/** "file", "project" or the id of a registered tab (`registerProblemsTab`). */
export type ProblemsTab = "file" | "project" | (string & {});

const tab = (): ProblemsTab => {
  const t = toolWindowTab("problems", "file");
  return t === "project" || problemsTabs().some((x) => x.id === t) ? t : "file";
};
const setTab = (t: ProblemsTab) => setToolWindowTab("problems", t);
const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
const [selected, setSelected] = createSignal<string | null>(null);

export { setTab as setProblemsTab };

const toggle = (path: string) =>
  setCollapsed((prev) => {
    const next = new Set(prev);
    next.has(path) ? next.delete(path) : next.add(path);
    return next;
  });

export const expandAllProblems = () => setCollapsed(new Set<string>());
export const collapseAllProblems = () => setCollapsed(new Set(Object.keys(diagnostics)));

function ProblemRow(props: { path: string; diag: EditorDiagnostic; index: number }) {
  const key = () => `${props.path}#${props.index}`;
  const pos = () => props.diag.range.start;
  return (
    <div
      class="tree-row problem-row"
      classList={{ selected: selected() === key() }}
      style={{ "padding-left": "38px" }}
      data-problem={props.diag.message}
      onClick={() => {
        setSelected(key());
        void revealLocation(props.path, pos(), props.diag.range.end);
      }}
      onKeyDown={(e) => e.key === "Enter" && void revealLocation(props.path, pos(), props.diag.range.end)}
      tabIndex={0}
    >
      <span class={`problem-icon sev-${props.diag.severity}`}>
        <Dynamic component={severityIcon[props.diag.severity]} size={14} />
      </span>
      <span class="problem-message" classList={{ "problem-unused": props.diag.unnecessary }}>
        {props.diag.message}
      </span>
      <span class="tree-hint">
        :{pos().line + 1}
      </span>
    </div>
  );
}

function FileNode(props: { path: string; diags: EditorDiagnostic[] }) {
  const open = () => !collapsed().has(props.path);
  const dir = () => props.path.split("/").slice(0, -1).join("/");
  return (
    <>
      <div class="tree-row problem-file" style={{ "padding-left": "8px" }} onClick={() => toggle(props.path)}>
        <span class="tree-chevron">{open() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</span>
        <FileIcon name={baseName(props.path)} />
        <span class="tree-label">{baseName(props.path)}</span>
        <span class="tree-hint">{dir()}</span>
        <span class="tree-hint">{pluralize(props.diags.length, "problem")}</span>
      </div>
      <Show when={open()}>
        <For each={props.diags}>{(d, i) => <ProblemRow path={props.path} diag={d} index={i()} />}</For>
      </Show>
    </>
  );
}

function Empty(props: { text: string }) {
  return <div class="problems-empty">{props.text}</div>;
}

export function ProblemsView() {
  const current = () => activeTab()?.path;
  const currentDiags = createMemo(() => {
    const p = current();
    return p ? sortDiagnostics(diagnostics[p] ?? []) : [];
  });
  const projectErrors = createMemo(() =>
    filesWithProblems(diagnostics)
      .map((path) => ({ path, diags: sortDiagnostics((diagnostics[path] ?? []).filter((d) => d.severity === "error")) }))
      .filter((f) => f.diags.length > 0),
  );
  const projectErrorCount = () => projectErrors().reduce((n, f) => n + f.diags.length, 0);

  return (
    <div class="problems-view">
      <div class="problems-tabs" role="tablist">
        <button class="problems-tab" classList={{ active: tab() === "file" }} role="tab" onClick={() => setTab("file")}>
          {t("problems.currentFile")}
          <Show when={currentDiags().length}>
            <span class="problems-count">{currentDiags().length}</span>
          </Show>
        </button>
        <button
          class="problems-tab"
          classList={{ active: tab() === "project" }}
          role="tab"
          onClick={() => setTab("project")}
        >
          {t("problems.projectErrors")}
          <Show when={projectErrorCount()}>
            <span class="problems-count">{projectErrorCount()}</span>
          </Show>
        </button>
        <For each={problemsTabs()}>
          {(pt) => (
            <button
              class="problems-tab"
              classList={{ active: tab() === pt.id }}
              role="tab"
              data-problems-tab={pt.id}
              onClick={() => setTab(pt.id)}
            >
              {pt.title}
              <Show when={pt.count?.()}>
                <span class="problems-count">{pt.count?.()}</span>
              </Show>
            </button>
          )}
        </For>
      </div>
      <div class="problems-tree">
        <Show when={tab() === "file"}>
          <Show when={current()} fallback={<Empty text={t("problems.noFile")} />}>
            {(path) => (
              <Show when={currentDiags().length} fallback={<Empty text={`No problems in ${baseName(path())}`} />}>
                <FileNode path={path()} diags={currentDiags()} />
              </Show>
            )}
          </Show>
        </Show>
        <Show when={tab() === "project"}>
          <Show when={projectErrors().length} fallback={<Empty text={t("problems.noErrors")} />}>
            <For each={projectErrors()}>{(f) => <FileNode path={f.path} diags={f.diags} />}</For>
          </Show>
        </Show>
        <For each={problemsTabs()}>{(t) => <Show when={tab() === t.id}>
              <Dynamic component={t.component} />
            </Show>}</For>
      </div>
    </div>
  );
}
