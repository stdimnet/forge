/** Test results tree of a test run (PHPUnit, Pest): statuses, durations, navigation. */
import { createMemo, For, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { Ban, ChevronDown, ChevronRight, CircleCheck, CircleX, LoaderCircle } from "lucide-solid";
import { workspaceApi } from "@/ipc/workspace";
import { workspace } from "@/state/workspace";
import { revealLocation } from "@/features/lsp/navigation";
import { showBalloon } from "@/keymap";
import { formatDuration, suiteStatus, type TestNode, type TestStatus } from "./teamcity";
import { toWorkspacePath } from "./fileLinks";

export interface TreeView {
  showPassed: boolean;
  showIgnored: boolean;
  sort: "none" | "alpha" | "duration";
  expanded: Set<number>;
  collapsed: Set<number>;
}

export const statusOf = (n: TestNode): TestStatus => (n.kind === "test" ? n.status : n.children.length ? suiteStatus(n) : n.status);

export function StatusIcon(props: { status: TestStatus; size?: number }) {
  const icon = () =>
    ({ passed: CircleCheck, failed: CircleX, ignored: Ban, running: LoaderCircle, pending: LoaderCircle })[props.status];
  return (
    <span class={`test-status test-${props.status}`}>
      <Dynamic component={icon()} size={props.size ?? 14} />
    </span>
  );
}

export interface FlatRow {
  node: TestNode;
  depth: number;
  open: boolean;
}

/** Visible rows: filters and sorting applied; suites are open unless collapsed. */
export function flattenTree(root: TestNode, view: TreeView): FlatRow[] {
  const rows: FlatRow[] = [];
  const visible = (n: TestNode): boolean => {
    if (n.kind !== "test") return n.children.some(visible) || n.children.length === 0;
    const s = n.status;
    if (s === "passed" && !view.showPassed) return false;
    if (s === "ignored" && !view.showIgnored) return false;
    return true;
  };
  const sorted = (list: TestNode[]) => {
    if (view.sort === "alpha") return [...list].sort((a, b) => a.name.localeCompare(b.name));
    if (view.sort === "duration") return [...list].sort((a, b) => (b.duration ?? 0) - (a.duration ?? 0));
    return list;
  };
  const walk = (n: TestNode, depth: number) => {
    const open = n.kind !== "test" && !view.collapsed.has(n.id);
    rows.push({ node: n, depth, open });
    if (open) for (const c of sorted(n.children)) if (visible(c)) walk(c, depth + 1);
  };
  walk(root, 0);
  return rows;
}

/** Open the test's source: the method or class named in its location hint. */
export async function navigateToTest(node: TestNode): Promise<void> {
  const loc = node.location ?? node.parent?.location;
  if (!loc) return;
  const rel = toWorkspacePath(loc.path, workspace()?.root);
  if (!rel) return void showBalloon(`${loc.path} is outside the project`);
  let line = loc.line ? loc.line - 1 : 0;
  let character = 0;
  if (!loc.line && loc.member) {
    const text = await workspaceApi.readFile(rel).catch(() => "");
    const short = loc.member.split("\\").pop()!;
    const re = node.kind === "test" ? new RegExp(`function\\s+${short}\\s*\\(`) : new RegExp(`(class|trait)\\s+${short}\\b`);
    const lines = text.split("\n");
    const idx = lines.findIndex((l) => re.test(l));
    if (idx >= 0) {
      line = idx;
      character = Math.max(0, lines[idx].indexOf(short));
    }
  }
  await revealLocation(rel, { line, character });
}

export function TestTree(props: {
  root: TestNode;
  version: number;
  view: TreeView;
  selected: number | null;
  onSelect: (n: TestNode) => void;
  onToggle: (n: TestNode) => void;
}) {
  const rows = createMemo(() => {
    props.version;
    return flattenTree(props.root, props.view);
  });
  const move = (delta: number) => {
    const list = rows();
    const idx = list.findIndex((r) => r.node.id === props.selected);
    const next = list[Math.max(0, Math.min(list.length - 1, idx + delta))];
    if (next) props.onSelect(next.node);
  };
  return (
    <div
      class="test-tree"
      tabIndex={0}
      role="tree"
      onKeyDown={(e) => {
        const sel = rows().find((r) => r.node.id === props.selected);
        if (e.key === "ArrowDown") move(1);
        else if (e.key === "ArrowUp") move(-1);
        else if (e.key === "ArrowRight" && sel && !sel.open && sel.node.kind !== "test") props.onToggle(sel.node);
        else if (e.key === "ArrowLeft" && sel?.open && sel.node.kind !== "root") props.onToggle(sel.node);
        else if ((e.key === "Enter" || e.key === "F4") && sel) void navigateToTest(sel.node);
        else return;
        e.preventDefault();
      }}
    >
      <For each={rows()}>
        {(row) => (
          <div
            class="tree-row test-row"
            classList={{ selected: props.selected === row.node.id }}
            role="treeitem"
            data-test={row.node.name}
            style={{ "padding-left": `${6 + row.depth * 16}px` }}
            onClick={() => props.onSelect(row.node)}
            onDblClick={() => (row.node.kind === "test" ? void navigateToTest(row.node) : props.onToggle(row.node))}
          >
            <span class="test-chevron" onClick={(e) => (e.stopPropagation(), props.onToggle(row.node))}>
              <Show when={row.node.kind !== "test" && row.node.children.length}>
                {row.open ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
              </Show>
            </span>
            <StatusIcon status={statusOf(row.node)} />
            <span class="test-name">{row.node.kind === "root" ? "Test Results" : row.node.name}</span>
            <span class="test-duration">{formatDuration(row.node.duration)}</span>
          </div>
        )}
      </For>
    </div>
  );
}
