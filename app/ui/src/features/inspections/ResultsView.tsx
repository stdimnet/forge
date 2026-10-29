/**
 * Inspection Results tool window, like PHPStorm's: a tab per run, the problems grouped by
 * inspection group, inspection and file (optionally by severity and directory), the
 * selected problem with its quick fixes and Suppress actions, the duplicate groups, Rerun
 * and Export to HTML / XML. The tree is computed in Rust (`forge_inspections::report`).
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import {
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  CircleCheck,
  Copy,
  Download,
  FolderTree,
  ListTree,
  RotateCw,
  SlidersHorizontal,
  TriangleAlert,
  X,
} from "lucide-solid";
import type { Grouping, ReportProblem, ResultNode } from "@/ipc/inspections";
import { workspaceApi } from "@/ipc/workspace";
import { openFileAt } from "@/editor/views";
import { FileIcon } from "@/shell/FileIcon";
import { showContextMenu, item, type MenuEntry } from "@/features/contextMenu/state";
import { openSettings } from "@/features/settings/dialogState";
import { severityIcon } from "@/features/lsp/severity";
import { INSPECTIONS_PAGE } from "./draft";
import { reportTree, suppressActions } from "./model";
import {
  activeResultsTab,
  applyFix,
  applySuppression,
  closeResultsTab,
  exportResults,
  rerun,
  resultTabs,
  setActiveResultsTab,
  type ResultsTab,
} from "./run";

const [bySeverity, setBySeverity] = createSignal(false);
const [byDirectory, setByDirectory] = createSignal(false);
const [mode, setMode] = createSignal<"problems" | "duplicates">("problems");
const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
const [selection, setSelection] = createSignal<{ tab: number; key: string; problem?: number; group?: number } | null>(null);

const current = (): ResultsTab | undefined => resultTabs().find((t) => t.id === activeResultsTab()) ?? resultTabs().at(-1);

const levels = (): Grouping[] => [
  ...(bySeverity() ? (["severity"] as Grouping[]) : []),
  "group",
  "inspection",
  ...(byDirectory() ? (["directory"] as Grouping[]) : []),
  "file",
];

const baseName = (p: string) => p.split("/").pop() ?? p;

function open(p: ReportProblem): void {
  void openFileAt(p.path, p.line + 1, p.column + 1);
}

function allKeys(nodes: ResultNode[], out: string[] = []): string[] {
  for (const n of nodes) {
    if (n.children.length) {
      out.push(n.key);
      allKeys(n.children, out);
    }
  }
  return out;
}

export function ResultsTabs() {
  return (
    <div class="run-tabs" role="tablist">
      <For each={resultTabs()}>
        {(t) => (
          <div
            class="run-tab"
            classList={{ active: current()?.id === t.id }}
            role="tab"
            onClick={() => setActiveResultsTab(t.id)}
            onMouseDown={(e) => e.button === 1 && (e.preventDefault(), closeResultsTab(t.id))}
          >
            <span class="run-tab-title">{t.title}</span>
            <button
              class="run-tab-close"
              title="Close Tab"
              onClick={(e) => {
                e.stopPropagation();
                closeResultsTab(t.id);
              }}
            >
              <X size={12} />
            </button>
          </div>
        )}
      </For>
    </div>
  );
}

function Node(props: { tab: ResultsTab; node: ResultNode; depth: number }) {
  const isCollapsed = () => collapsed().has(props.node.key);
  const leaf = () => props.node.problem !== undefined;
  const problem = () => (leaf() ? props.tab.report.problems[props.node.problem!] : undefined);
  const resolved = () => leaf() && props.tab.resolved.includes(props.node.problem!);
  const select = () => setSelection({ tab: props.tab.id, key: props.node.key, problem: props.node.problem });
  const toggle = () =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      next.has(props.node.key) ? next.delete(props.node.key) : next.add(props.node.key);
      return next;
    });
  return (
    <>
      <div
        class="ir-row"
        classList={{ selected: selection()?.key === props.node.key, resolved: resolved(), [`ir-${props.node.kind}`]: true }}
        style={{ "padding-left": `${6 + props.depth * 16}px` }}
        tabIndex={0}
        data-node={props.node.kind}
        onClick={() => {
          select();
          if (!leaf()) toggle();
        }}
        onDblClick={() => {
          const p = problem();
          if (p) open(p);
        }}
        onKeyDown={(e) => {
          const p = problem();
          if (e.key === "Enter" && p) open(p);
        }}
      >
        <Show when={!leaf()} fallback={<span class="ir-chevron" />}>
          <span class="ir-chevron">{isCollapsed() ? <ChevronRight size={13} /> : <ChevronDown size={13} />}</span>
        </Show>
        <Show when={props.node.kind === "file"}>
          <FileIcon name={props.node.label} />
        </Show>
        <Show when={leaf() || props.node.kind === "severity" || props.node.kind === "inspection"}>
          <span class={`problem-icon sev-${props.node.editor}`}>
            <Dynamic component={severityIcon[props.node.editor]} size={13} />
          </span>
        </Show>
        <span class="ir-label" classList={{ "ir-resolved": resolved() }}>
          {props.node.label}
        </span>
        <Show when={leaf()} fallback={<span class="ir-count">{props.node.count}</span>}>
          <span class="ir-loc">
            {baseName(problem()!.path)}:{problem()!.line + 1}
          </span>
        </Show>
        <Show when={props.node.kind === "file" || props.node.kind === "directory"}>
          <span class="ir-dir">{props.node.detail}</span>
        </Show>
      </div>
      <Show when={!leaf() && !isCollapsed()}>
        <For each={props.node.children}>{(c) => <Node tab={props.tab} node={c} depth={props.depth + 1} />}</For>
      </Show>
    </>
  );
}

async function suppressMenu(tab: ResultsTab, index: number, e: MouseEvent): Promise<void> {
  const p = tab.report.problems[index];
  const text = await workspaceApi.readFile(p.path).catch(() => null);
  if (text === null) return;
  const actions = suppressActions(p.path, text, p.line, p.column, p.inspection, true);
  const entries: MenuEntry[] = actions.map((a) => item(a.title, () => void applySuppression(tab, index, a.changes[0])));
  showContextMenu(entries, e.clientX, e.clientY);
}

function ProblemDetails(props: { tab: ResultsTab; index: number }) {
  const p = () => props.tab.report.problems[props.index];
  const resolved = () => props.tab.resolved.includes(props.index);
  return (
    <div class="ir-details" data-problem={p().inspection}>
      <div class="ir-details-title">
        <span class={`problem-icon sev-${p().editor}`}>
          <Dynamic component={severityIcon[p().editor]} size={14} />
        </span>
        {p().inspectionName}
        <span class="ir-sev">{p().severityName}</span>
      </div>
      <div class="ir-details-message">{p().message}</div>
      <button class="link-btn ir-location" onClick={() => open(p())}>
        {p().path}:{p().line + 1}
      </button>
      <pre class="ir-snippet">{p().snippet}</pre>
      <Show when={p().related?.length}>
        <div class="ir-related-title">Other copies</div>
        <For each={p().related}>
          {(r) => (
            <button class="link-btn ir-location" onClick={() => void openFileAt(r.path, r.line + 1, r.column + 1)}>
              {r.path}:{r.line + 1}–{r.endLine + 1}
            </button>
          )}
        </For>
      </Show>
      <Show when={resolved()}>
        <div class="ir-resolved-note">
          <CircleCheck size={13} /> Resolved. Rerun the inspection to refresh the results.
        </div>
      </Show>
      <div class="ir-actions">
        <For each={p().fixes ?? []}>
          {(fix) => (
            <button class="btn-primary" disabled={resolved()} onClick={() => void applyFix(props.tab, props.index, fix)}>
              {fix.title}
            </button>
          )}
        </For>
        <button class="btn-secondary" disabled={resolved()} onClick={(e) => void suppressMenu(props.tab, props.index, e)}>
          Suppress…
        </button>
        <button class="btn-secondary" onClick={() => openSettings(INSPECTIONS_PAGE, { inspection: p().inspection })}>
          Edit Inspection Profile Setting
        </button>
      </div>
    </div>
  );
}

function DuplicatesView(props: { tab: ResultsTab }) {
  return (
    <div class="ir-dups">
      <For each={props.tab.report.duplicates} fallback={<div class="ir-empty">No duplicates found</div>}>
        {(g, i) => (
          <div class="ir-dup" classList={{ selected: selection()?.group === i() }}>
            <div class="ir-row ir-dup-head" onClick={() => setSelection({ tab: props.tab.id, key: `dup${i()}`, group: i() })}>
              <Copy size={13} class="muted" />
              <span class="ir-label">
                Duplicated code: {g.lines} lines, {g.tokens} tokens
              </span>
              <span class="ir-count">{g.fragments.length} copies</span>
            </div>
            <For each={g.fragments}>
              {(f) => (
                <div class="ir-row ir-dup-fragment" onDblClick={() => void openFileAt(f.path, f.line + 1, f.column + 1)} onClick={() => void openFileAt(f.path, f.line + 1, f.column + 1, { focus: false })}>
                  <FileIcon name={baseName(f.path)} />
                  <span class="ir-label">{baseName(f.path)}</span>
                  <span class="ir-dir">
                    {f.path} lines {f.line + 1}–{f.endLine + 1}
                  </span>
                </div>
              )}
            </For>
          </div>
        )}
      </For>
    </div>
  );
}

export function ResultsView() {
  const tree = createMemo(() => {
    const t = current();
    return t ? reportTree(t.report, levels()) : [];
  });
  const selectedProblem = () => {
    const s = selection();
    const t = current();
    return s && t && s.tab === t.id && s.problem !== undefined ? s.problem : undefined;
  };
  const exportMenu = (t: ResultsTab, e: MouseEvent) =>
    showContextMenu([item("Export to HTML…", () => void exportResults(t, "html")), item("Export to XML…", () => void exportResults(t, "xml"))], e.clientX, e.clientY);
  return (
    <div class="ir-window">
      <Show
        when={current()}
        fallback={
          <div class="ir-empty">
            <ListTree size={28} class="muted" />
            <div>Run Code | Inspect Code… to see results here</div>
          </div>
        }
      >
        {(t) => (
          <>
            <div class="ir-toolbar" role="toolbar">
              <button class="ir-tool" title="Rerun Inspection" aria-label="Rerun Inspection" onClick={() => rerun(t())}>
                <RotateCw size={15} />
              </button>
              <button class="ir-tool" classList={{ on: bySeverity() }} title="Group by Severity" aria-pressed={bySeverity()} onClick={() => setBySeverity(!bySeverity())}>
                <TriangleAlert size={15} />
              </button>
              <button class="ir-tool" classList={{ on: byDirectory() }} title="Group by Directory" aria-pressed={byDirectory()} onClick={() => setByDirectory(!byDirectory())}>
                <FolderTree size={15} />
              </button>
              <button class="ir-tool" title="Expand All" onClick={() => setCollapsed(new Set<string>())}>
                <ChevronsUpDown size={15} />
              </button>
              <button class="ir-tool" title="Collapse All" onClick={() => setCollapsed(new Set(allKeys(tree())))}>
                <ChevronsDownUp size={15} />
              </button>
              <button class="ir-tool" title="Export" aria-label="Export" onClick={(e) => exportMenu(t(), e)}>
                <Download size={15} />
              </button>
              <button class="ir-tool" title="Edit Settings" aria-label="Edit Inspection Settings" onClick={() => openSettings(INSPECTIONS_PAGE)}>
                <SlidersHorizontal size={15} />
              </button>
            </div>
            <div class="ir-main">
              <div class="ir-header">
                <div class="ir-modes" role="tablist">
                  <button class="ir-mode" classList={{ active: mode() === "problems" }} onClick={() => setMode("problems")}>
                    Problems <span class="ir-count">{t().report.problems.length}</span>
                  </button>
                  <button class="ir-mode" classList={{ active: mode() === "duplicates" }} onClick={() => setMode("duplicates")}>
                    Duplicates <span class="ir-count">{t().report.duplicates.length}</span>
                  </button>
                </div>
                <span class="ir-summary">
                  {t().report.files} files · profile '{t().report.profile}'{t().report.cancelled ? " · cancelled" : ""}
                </span>
              </div>
              <Show when={t().report.errors.length}>
                <div class="ir-errors">{t().report.errors.join("; ")}</div>
              </Show>
              <div class="ir-split">
                <div class="ir-tree" role="tree">
                  <Show when={mode() === "problems"} fallback={<DuplicatesView tab={t()} />}>
                    <For each={tree()} fallback={<div class="ir-empty">No problems found</div>}>
                      {(n) => <Node tab={t()} node={n} depth={0} />}
                    </For>
                  </Show>
                </div>
                <div class="ir-side">
                  <Show when={selectedProblem() !== undefined} fallback={<div class="ir-empty ir-hint">Select a problem to see its description and fixes</div>}>
                    <ProblemDetails tab={t()} index={selectedProblem()!} />
                  </Show>
                </div>
              </div>
            </div>
          </>
        )}
      </Show>
    </div>
  );
}
