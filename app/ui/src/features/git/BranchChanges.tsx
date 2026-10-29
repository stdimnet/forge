/**
 * Compare with Branch on a folder: the files that differ between a branch and the working
 * tree, in an editor tab. Double-click (or Ctrl+D) shows the file diff.
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { Columns2, GitCompare, RefreshCw } from "lucide-solid";
import { openCustomTab } from "@/state/editor";
import { FileIcon } from "@/shell/FileIcon";
import { registerCompareHandler } from "@/features/compare/state";
import type { ChangedFile } from "./api";
import { historyApi } from "./historyApi";
import { showRevisionDiff } from "./actions";

const STATUS: Record<string, { label: string; cls: string }> = {
  A: { label: "Only in the working tree", cls: "vcs-added" },
  D: { label: "Only on the branch", cls: "vcs-deleted" },
  R: { label: "Renamed", cls: "vcs-modified" },
  M: { label: "Modified", cls: "vcs-modified" },
};

let focusedDiff: (() => void) | null = null;
registerCompareHandler(".branch-changes", () => focusedDiff?.());

function BranchChangesView(props: { branch: string; path: string; label: string }) {
  const [changes, { refetch }] = createResource(() => historyApi.changesSince(props.branch, props.path));
  const [selected, setSelected] = createSignal<string | null>(null);
  const current = () => changes()?.find((c) => c.path === selected());
  const open = (c: ChangedFile | undefined) => {
    if (c) void showRevisionDiff(c.path, props.branch, props.label, c.origPath ?? c.path);
  };
  return (
    <div class="dir-diff branch-changes" onFocusIn={() => (focusedDiff = () => open(current()))}>
      <div class="panel-toolbar dir-diff-toolbar">
        <button class="icon-btn" title="Refresh" onClick={() => void refetch()}>
          <RefreshCw size={15} />
        </button>
        <button class="icon-btn" title="Show Diff (Ctrl+D)" disabled={!current()} onClick={() => open(current())}>
          <Columns2 size={15} />
        </button>
        <span class="spacer" />
        <span class="diff-count">
          {changes() ? `${changes()!.length} file${changes()!.length === 1 ? "" : "s"} differ` : changes.error ? "Comparison failed" : "Comparing…"}
        </span>
      </div>
      <div class="dir-diff-header branch-changes-header">
        <GitCompare size={14} class="muted" />
        <span>
          Files that differ between <b>{props.label}</b> and the working tree in <b>{props.path || "/"}</b>
        </span>
      </div>
      <div class="dir-diff-list" tabIndex={0}>
        <Show when={changes.error}>
          <div class="commit-empty">{String(changes.error instanceof Error ? changes.error.message : changes.error)}</div>
        </Show>
        <For each={changes() ?? []}>
          {(c) => {
            const s = STATUS[c.status] ?? STATUS.M;
            return (
              <div
                class="tree-row"
                classList={{ selected: selected() === c.path }}
                style={{ "padding-left": "12px" }}
                title={s.label}
                onClick={() => setSelected(c.path)}
                onDblClick={() => open(c)}
              >
                <FileIcon name={c.path.split("/").pop()!} />
                <span class={`tree-label ${s.cls}`}>{c.path.split("/").pop()}</span>
                <span class="tree-hint">
                  {c.path.split("/").slice(0, -1).join("/")}
                  {c.origPath ? ` (from ${c.origPath})` : ""}
                </span>
              </div>
            );
          }}
        </For>
        <Show when={changes() && !changes()!.length}>
          <div class="commit-empty">No differences</div>
        </Show>
      </div>
    </div>
  );
}

export function openBranchChanges(branch: string, path: string, label = branch): void {
  const name = path.split("/").pop() || "Project";
  openCustomTab({
    id: `forge:branch-changes:${branch}\u0000${path}`,
    title: `${name} vs ${label}`,
    tooltip: `${path || "/"} compared with ${label}`,
    icon: GitCompare,
    component: () => <BranchChangesView branch={branch} path={path} label={label} />,
  });
}
