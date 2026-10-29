/**
 * Coverage tool window (PHPStorm: View | Tool Windows | Coverage): the shown suite's
 * directories and files with their line and branch coverage, computed in Rust
 * (`forge_testing::coverage::summary`), plus method coverage when the report names
 * functions. Double-click opens a file; the toolbar shows / hides the coverage in the editor
 * and the Project view, chooses (and merges) suites, imports a report, generates the HTML
 * report and closes the suite.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { ChevronDown, ChevronRight, Eye, EyeOff, File, FileCode2, FileInput, Folder, History, X } from "lucide-solid";
import { openFileAt } from "@/editor/views";
import { shortcutLabel } from "@/keymap";
import type { CoverageNode } from "@/ipc/testing";
import { activeSuite, closeSuite, setCoverageVisible, view, visible } from "./store";
import { generateReport, importReport } from "./actions";
import { showSuitesDialog, when } from "./SuitesDialog";

function Percent(props: { text: string; covered: number; total: number }) {
  const pct = () => (props.total ? (100 * props.covered) / props.total : 0);
  return (
    <span class="coverage-pct">
      <Show when={props.total} fallback={<span class="muted">n/a</span>}>
        <span class="coverage-bar">
          <span class="coverage-bar-fill" style={{ width: `${pct()}%` }} />
        </span>
        <span>{props.text}</span>
      </Show>
    </span>
  );
}

export function CoverageToolWindow() {
  const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
  const [selected, setSelected] = createSignal<string | null>(null);
  const rows = createMemo(() => {
    const nodes = view()?.nodes ?? [];
    const out: CoverageNode[] = [];
    let hideBelow: number | null = null;
    for (const n of nodes) {
      if (hideBelow !== null && n.depth > hideBelow) continue;
      hideBelow = null;
      out.push(n);
      if (n.kind !== "file" && collapsed().has(n.path)) hideBelow = n.depth;
    }
    return out;
  });
  const toggle = (n: CoverageNode) =>
    setCollapsed((c) => {
      const next = new Set(c);
      if (next.has(n.path)) next.delete(n.path);
      else next.add(n.path);
      return next;
    });
  const open = (n: CoverageNode) => (n.kind === "file" ? void openFileAt(n.path, 1, 1) : toggle(n));
  // The Method column only for reports that name functions (not Go profiles, luacov).
  const methods = () => (view()?.nodes[0]?.methods.total ?? 0) > 0;
  return (
    <div class="coverage-window">
      <div class="coverage-toolbar" role="toolbar">
        <Show when={activeSuite()} fallback={<span class="muted coverage-title">No coverage data</span>}>
          {(s) => (
            <span class="coverage-title" title={`${s().format} report`}>
              <b>{s().name}</b>
              <span class="muted">
                {" "}
                · {s().format} · {when(s().at)}
              </span>
            </span>
          )}
        </Show>
        <span class="coverage-spacer" />
        <button
          class="icon-btn coverage-toggle"
          title={visible() ? "Hide Coverage" : "Show Coverage"}
          disabled={!activeSuite()}
          onClick={() => setCoverageVisible(!visible())}
        >
          {visible() ? <EyeOff size={15} /> : <Eye size={15} />}
        </button>
        <button
          class="icon-btn"
          title={`Show Coverage Data…${shortcutLabel("SwitchCoverage") ? ` (${shortcutLabel("SwitchCoverage")})` : ""}`}
          onClick={() => showSuitesDialog()}
        >
          <History size={15} />
        </button>
        <button class="icon-btn" title="Import External Coverage Report…" onClick={() => void importReport()}>
          <FileInput size={15} />
        </button>
        <button class="icon-btn" title="Generate Coverage Report…" disabled={!activeSuite()} onClick={() => void generateReport()}>
          <FileCode2 size={15} />
        </button>
        <button class="icon-btn" title="Close Coverage Suite" disabled={!activeSuite()} onClick={() => activeSuite() && void closeSuite(activeSuite()!.id)}>
          <X size={15} />
        </button>
      </div>
      <Show
        when={view()}
        fallback={
          <div class="coverage-empty muted">
            <p>No coverage data.</p>
            <p>Run tests with coverage from the gutter or with Run | Run with Coverage, or import a coverage report.</p>
          </div>
        }
      >
        <div class="coverage-table" classList={{ "with-methods": methods() }} role="treegrid">
          <div class="coverage-head">
            <span>Element</span>
            <Show when={methods()}>
              <span>Method, %</span>
            </Show>
            <span>Line, %</span>
            <span>Branch, %</span>
          </div>
          <div class="coverage-body">
            <For each={rows()}>
              {(n) => (
                <div
                  class="coverage-row"
                  classList={{ selected: selected() === n.path, file: n.kind === "file" }}
                  role="row"
                  data-coverage-path={n.path}
                  onClick={() => setSelected(n.path)}
                  onDblClick={() => open(n)}
                >
                  <span class="coverage-name" style={{ "padding-left": `${6 + n.depth * 16}px` }}>
                    <span class="coverage-chevron" onClick={(e) => (e.stopPropagation(), n.kind !== "file" && toggle(n))}>
                      <Show when={n.kind !== "file"}>{collapsed().has(n.path) ? <ChevronRight size={13} /> : <ChevronDown size={13} />}</Show>
                    </span>
                    {n.kind === "file" ? <File size={14} class="coverage-icon" /> : <Folder size={14} class="coverage-icon folder" />}
                    <span>{n.kind === "project" ? n.name || "All files" : n.name}</span>
                  </span>
                  <Show when={methods()}>
                    <Percent text={n.methodText} covered={n.methods.covered} total={n.methods.total} />
                  </Show>
                  <Percent text={n.lineText} covered={n.lines.covered} total={n.lines.total} />
                  <Percent text={n.branchText} covered={n.branches.covered} total={n.branches.total} />
                </div>
              )}
            </For>
          </div>
        </div>
      </Show>
    </div>
  );
}
