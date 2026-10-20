/**
 * Services tool window (Alt+8), like PHPStorm's: a vertical action toolbar, the tree
 * (Docker → Containers / Images / Networks / Volumes, Docker Compose → projects → services,
 * Kubernetes → contexts → namespaces → workloads) and the details pane of the selected
 * node. The tree and the details are computed by the backend.
 */
import { createMemo, createResource, createRoot, createSignal, For, Show, type JSX } from "solid-js";
import { Dynamic } from "solid-js/web";
import {
  ArrowRightLeft,
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  Download,
  LoaderCircle,
  Play,
  RefreshCw,
  RotateCw,
  ScrollText,
  Square,
  SquareTerminal,
  Trash2,
} from "lucide-solid";
import { navigate, type VisibleRow } from "@/features/project/treeNavigation";
import { openMenu, type MenuItem } from "@/features/run/PopupMenu";
import { servicesApi, type ServiceAction, type ServiceDetails, type ServiceNode } from "@/ipc/services";
import { DetailsPane } from "./DetailsPane";
import { ForwardPortsDialog } from "./ForwardPortsDialog";
import { NodeIconView } from "./icons";
import {
  collapseAll,
  containerOf,
  expandAll,
  expanded,
  findNode,
  loading,
  nodes,
  openExec,
  pullImageDialog,
  refresh,
  runAction,
  select,
  selected,
  setOpen,
  setTab,
  version,
} from "./store";
import "./services.css";

type IconComp = (p: { size?: number; "stroke-width"?: number; class?: string; fill?: string }) => JSX.Element;

interface Row {
  node: ServiceNode;
  depth: number;
  expandable: boolean;
}

function visibleRows(): Row[] {
  const out: Row[] = [];
  const open = expanded();
  const walk = (list: ServiceNode[], depth: number) => {
    for (const node of list) {
      const expandable = !!node.lazy || !!node.children?.length;
      out.push({ node, depth, expandable });
      if (expandable && open.has(node.id) && node.children) walk(node.children, depth + 1);
    }
  };
  walk(nodes(), 0);
  return out;
}

const [detailsResource] = createRoot(() =>
  createResource(
    () => [selected(), version()] as const,
    async ([id]) => (id ? servicesApi.details(id).catch(() => null) : null),
  ),
);
/** Details of the selected node (the previous answer while the next one loads). */
export const details = () => detailsResource.latest;

interface ToolButton {
  id: string;
  title: string;
  icon: IconComp;
  cls?: string;
  /** The backend actions this button stands for, first match wins. */
  actions: ServiceAction[];
}

const TOOL_BUTTONS: (ToolButton | "sep")[] = [
  { id: "start", title: "Start", icon: Play, cls: "run", actions: ["start", "up", "connect"] },
  { id: "stop", title: "Stop", icon: Square, cls: "stop", actions: ["stop"] },
  { id: "restart", title: "Restart", icon: RotateCw, actions: ["restart"] },
  { id: "remove", title: "Delete", icon: Trash2, actions: ["remove", "down"] },
  "sep",
  { id: "logs", title: "Show Log", icon: ScrollText, actions: ["logs"] },
  { id: "exec", title: "Exec (open a terminal in the container)", icon: SquareTerminal, actions: ["exec"] },
  { id: "pull", title: "Pull Image", icon: Download, actions: ["pull"] },
  { id: "forward", title: "Forward Ports…", icon: ArrowRightLeft, actions: ["forwardPorts"] },
];

const LABELS: Partial<Record<ServiceAction, string>> = {
  up: "Up",
  down: "Down",
  connect: "Connect",
  start: "Start",
  remove: "Delete",
};

function actionFor(b: ToolButton, d: ServiceDetails | null | undefined): ServiceAction | undefined {
  return d ? b.actions.find((a) => d.actions.includes(a)) : undefined;
}

function Toolbar() {
  return (
    <div class="svc-toolbar" role="toolbar" aria-orientation="vertical">
      <For each={TOOL_BUTTONS}>
        {(b) =>
          b === "sep" ? (
            <span class="svc-tool-sep" />
          ) : (
            <button
              class={`icon-btn svc-tool ${b.cls ?? ""}`}
              data-action={b.id}
              disabled={!actionFor(b, details())}
              title={(() => {
                const a = actionFor(b, details());
                return a && LABELS[a] && b.id === "start" ? LABELS[a] : a === "down" ? "Down (docker compose down)" : b.title;
              })()}
              onClick={() => {
                const d = details();
                const a = actionFor(b, d);
                if (d && a) void runAction(d, a);
              }}
            >
              <Dynamic component={b.icon} size={15} stroke-width={b.cls ? 0 : 1.8} fill={b.cls ? "currentColor" : "none"} />
            </button>
          )
        }
      </For>
      <span class="svc-tool-sep" />
      <button class="icon-btn svc-tool" title="Refresh" onClick={() => void refresh()}>
        <RefreshCw size={14} />
      </button>
      <button class="icon-btn svc-tool" title="Expand All" onClick={expandAll}>
        <ChevronsUpDown size={14} />
      </button>
      <button class="icon-btn svc-tool" title="Collapse All" onClick={collapseAll}>
        <ChevronsDownUp size={14} />
      </button>
    </div>
  );
}

function contextMenu(node: ServiceNode, x: number, y: number) {
  void servicesApi.details(node.id).then((d) => {
    if (!d) return;
    const items: MenuItem[] = [];
    const add = (a: ServiceAction, label: string, icon: MenuItem["icon"], color?: string) => {
      if (d.actions.includes(a)) items.push({ label, icon, iconColor: color, run: () => void runAction(d, a) });
    };
    add("connect", "Connect", Play, "var(--success)");
    add("up", "Up", Play, "var(--success)");
    add("start", "Start Container", Play, "var(--success)");
    add("stop", d.target.kind === "container" ? "Stop Container" : "Stop", Square, "var(--danger)");
    add("restart", d.target.kind === "deployment" ? "Restart Rollout" : "Restart", RotateCw);
    add("down", "Down", Trash2);
    if (d.tabs.includes("log")) items.push({ label: "Show Log", icon: ScrollText, run: () => setTab("log") });
    if (d.actions.includes("exec")) items.push({ label: "Exec…", icon: SquareTerminal, run: () => void openExec(d.nodeId, containerOf(d)) });
    if (d.actions.includes("forwardPorts")) items.push({ label: "Forward Ports…", icon: ArrowRightLeft, run: () => void runAction(d, "forwardPorts") });
    if (d.tabs.includes("inspection")) items.push({ label: "Inspect", run: () => setTab("inspection") });
    if (d.actions.includes("pull"))
      items.push({ label: "Pull Image", icon: Download, run: () => (d.target.kind === "dockerGroup" ? pullImageDialog() : void runAction(d, "pull")) });
    if (d.actions.includes("remove")) items.push({ separator: true }, { label: "Delete…", icon: Trash2, run: () => void runAction(d, "remove") });
    items.push({ separator: true }, { label: "Refresh", icon: RefreshCw, run: () => (d.actions.includes("refresh") ? void runAction(d, "refresh") : void refresh()) });
    openMenu({ x, y, items });
  });
}

function Tree() {
  let root!: HTMLDivElement;
  const rows = createMemo(visibleRows);
  const activate = (row: Row) => {
    if (row.expandable) void setOpen(row.node, !expanded().has(row.node.id));
    else if (row.node.icon === "container" || row.node.icon === "pod" || row.node.icon === "service") setTab("log");
  };
  const onKeyDown = (e: KeyboardEvent) => {
    const list = rows();
    const visible: VisibleRow[] = list.map((r) => ({ path: r.node.id, isDir: r.expandable, expanded: expanded().has(r.node.id), depth: r.depth }));
    const action = navigate(visible, selected(), e.key);
    if (action.type === "none") return;
    e.preventDefault();
    const row = list.find((r) => r.node.id === action.path);
    if (action.type === "select") {
      select(action.path);
      root.querySelector(`[data-node-id="${CSS.escape(action.path)}"]`)?.scrollIntoView({ block: "nearest" });
    } else if (action.type === "expand" && row) void setOpen(row.node, true);
    else if (action.type === "collapse" && row) void setOpen(row.node, false);
    else if (row) activate(row);
  };
  return (
    <div class="svc-tree" tabIndex={0} ref={root} onKeyDown={onKeyDown} data-focus-context="toolwindow">
      <For each={rows()}>
        {(row) => (
          <div
            class="tree-row svc-row"
            classList={{ selected: selected() === row.node.id, muted: !!row.node.muted, error: row.node.icon === "error" }}
            style={{ "padding-left": `${6 + row.depth * 16}px` }}
            data-node-id={row.node.id}
            onClick={() => select(row.node.id)}
            onDblClick={() => activate(row)}
            onContextMenu={(e) => {
              e.preventDefault();
              select(row.node.id);
              contextMenu(row.node, e.clientX, e.clientY);
            }}
          >
            <span
              class="tree-chevron"
              onClick={(e) => {
                if (!row.expandable) return;
                e.stopPropagation();
                select(row.node.id);
                void setOpen(row.node, !expanded().has(row.node.id));
              }}
            >
              <Show when={row.expandable}>
                <Show when={!loading().has(row.node.id)} fallback={<LoaderCircle size={12} class="svc-spin" />}>
                  {expanded().has(row.node.id) ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                </Show>
              </Show>
            </span>
            <NodeIconView icon={row.node.icon} status={row.node.status} />
            <span class="tree-label" classList={{ strong: row.depth === 0 }}>
              {row.node.label}
            </span>
            <Show when={row.node.detail}>
              <span class="tree-hint">{row.node.detail}</span>
            </Show>
          </div>
        )}
      </For>
    </div>
  );
}

export function ServicesView() {
  const [treeWidth, setTreeWidth] = createSignal(340);
  const startResize = (e: MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const start = treeWidth();
    const move = (ev: MouseEvent) => setTreeWidth(Math.max(200, Math.min(700, start + ev.clientX - startX)));
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };
  return (
    <div class="svc-root" data-testid="services">
      <Toolbar />
      <div class="svc-tree-pane" style={{ width: `${treeWidth()}px` }}>
        <Tree />
      </div>
      <div class="splitter splitter-v" onMouseDown={startResize} />
      <div class="svc-details-pane">
        <Show when={details()?.nodeId} keyed fallback={<div class="svc-empty muted">{findNode(selected()) ? "Loading…" : "Select a service to view its details"}</div>}>
          {(_id) => <DetailsPane details={() => details()!} />}
        </Show>
      </div>
      <ForwardPortsDialog />
    </div>
  );
}
