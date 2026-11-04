/**
 * The Hierarchy tool window (PHPStorm's Type / Method / Call Hierarchy): one tab per run, a
 * toolbar with the view, scope and sorting, and the tree. Call Hierarchy tabs show the call
 * sites of the selected node below the tree; Method Hierarchy tabs the legend of the marks.
 */
import { createMemo, For, Show, type JSX } from "solid-js";
import {
  ArrowDownAZ,
  ArrowDownNarrowWide,
  ArrowUpNarrowWide,
  Check,
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  CircleAlert,
  Crosshair,
  ListFilter,
  Minus,
  Network,
  PhoneIncoming,
  PhoneOutgoing,
  RotateCw,
  X,
} from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { SymbolIcon } from "@/features/php/SymbolIcon";
import type { SymbolKind } from "@/features/php/api";
import { showResolvedMenu } from "@/features/contextMenu/state";
import { shortcutLabel } from "@/keymap";
import { navigate as treeNavigate } from "@/features/project/treeNavigation";
import type { HierarchyItem } from "@/ipc/hierarchy";
import { MARK_TITLES, SCOPES, type MethodMark, type TypeView } from "./model";
import { allKeys, rowsOf, type HierarchyRow } from "./rows";
import {
  activeHierarchyTab,
  autoscroll,
  closeHierarchyTab,
  hierarchyState,
  loadCalls,
  openItem,
  refreshTab,
  setActiveHierarchyTab,
  setAutoscroll,
  setCallDirection,
  toggleKey,
  updateTab,
  type HierarchyTab,
} from "./state";
import { t as tr } from "@/i18n";

const KIND_TITLES: Record<HierarchyTab["kind"], string> = {
  type: "Type Hierarchy",
  method: "Method Hierarchy",
  call: "Call Hierarchy",
};

export function HierarchyTabs() {
  return (
    <div class="run-tabs" role="tablist">
      <For each={hierarchyState.tabs}>
        {(t) => (
          <div
            class="run-tab"
            classList={{ active: hierarchyState.active === t.id }}
            role="tab"
            title={`${KIND_TITLES[t.kind]} of ${t.title}`}
            onClick={() => setActiveHierarchyTab(t.id)}
            onMouseDown={(e) => e.button === 1 && (e.preventDefault(), closeHierarchyTab(t.id))}
          >
            <span class="run-tab-title">{t.title}</span>
            <button
              class="run-tab-close"
              title={tr("common.closeTab")}
              onClick={(e) => {
                e.stopPropagation();
                closeHierarchyTab(t.id);
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

const SYMBOL_KINDS: Partial<Record<HierarchyItem["kind"], SymbolKind>> = {
  class: "class",
  interface: "interface",
  trait: "trait",
  enum: "enum",
  method: "method",
  constructor: "method",
  function: "function",
  property: "property",
};

function ItemIcon(props: { item: HierarchyItem }) {
  return (
    <Show when={SYMBOL_KINDS[props.item.kind]} fallback={<FileIcon name={props.item.name} size={14} />}>
      {(k) => <SymbolIcon kind={k()} modifiers={props.item.isAbstract ? { isAbstract: true } : undefined} />}
    </Show>
  );
}

function MarkIcon(props: { mark: MethodMark }) {
  return (
    <span class={`hierarchy-mark mark-${props.mark}`} title={MARK_TITLES[props.mark]}>
      {props.mark === "shouldDefine" ? (
        <CircleAlert size={13} />
      ) : props.mark === "inherited" ? (
        <Minus size={13} />
      ) : (
        <Check size={13} stroke-width={props.mark === "abstract" ? 1.6 : 2.6} />
      )}
    </span>
  );
}

function ToolButton(props: { title: string; on?: boolean; disabled?: boolean; onClick: (e: MouseEvent) => void; children: JSX.Element }) {
  return (
    <button
      class="hierarchy-tool"
      classList={{ on: !!props.on }}
      title={props.title}
      aria-label={props.title}
      aria-pressed={props.on === undefined ? undefined : props.on}
      disabled={props.disabled}
      onClick={(e) => props.onClick(e)}
    >
      {props.children}
    </button>
  );
}

function Toolbar(props: { tab: HierarchyTab }) {
  const id = () => props.tab.id;
  const setView = (view: TypeView) =>
    updateTab(id(), (t) => {
      if (t.kind === "type") {
        t.view = view;
        t.collapsed = [];
      }
    });
  const scopeMenu = (e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    showResolvedMenu(
      SCOPES.map((s) => ({
        kind: "action" as const,
        label: s.label,
        checkable: true,
        checked: props.tab.scope === s.id,
        run: () => updateTab(id(), (t) => (t.scope = s.id)),
      })),
      r.left,
      r.bottom + 2,
    );
  };
  const tab = () => props.tab;
  return (
    <div class="hierarchy-toolbar" role="toolbar">
      <Show when={tab().kind === "type" && tab()}>
        {(t) => {
          const view = () => (t() as { view: TypeView }).view;
          return (
            <>
              <ToolButton title={tr("tools.hierarchy.classHierarchy")} on={view() === "class"} onClick={() => setView("class")}>
                <Network size={15} />
              </ToolButton>
              <ToolButton title={tr("tools.hierarchy.supertypesHierarchy")} on={view() === "supertypes"} onClick={() => setView("supertypes")}>
                <ArrowUpNarrowWide size={15} />
              </ToolButton>
              <ToolButton title={tr("tools.hierarchy.subtypesHierarchy")} on={view() === "subtypes"} onClick={() => setView("subtypes")}>
                <ArrowDownNarrowWide size={15} />
              </ToolButton>
            </>
          );
        }}
      </Show>
      <Show when={tab().kind === "method" && tab()}>
        {(t) => {
          const hide = () => (t() as { hideNotImplemented: boolean }).hideNotImplemented;
          return (
            <ToolButton
              title={hide() ? tr("tools.hierarchy.showAllClasses") : tr("tools.hierarchy.hideClassesWhereMethodIs")}
              on={hide()}
              onClick={() =>
                updateTab(id(), (x) => {
                  if (x.kind === "method") x.hideNotImplemented = !x.hideNotImplemented;
                })
              }
            >
              <ListFilter size={15} />
            </ToolButton>
          );
        }}
      </Show>
      <Show when={tab().kind === "call" && tab()}>
        {(t) => {
          const dir = () => (t() as { direction: string }).direction;
          return (
            <>
              <ToolButton title={tr("tools.hierarchy.callerMethodsHierarchy")} on={dir() === "callers"} onClick={() => setCallDirection(id(), "callers")}>
                <PhoneIncoming size={15} />
              </ToolButton>
              <ToolButton title={tr("tools.hierarchy.calleeMethodsHierarchy")} on={dir() === "callees"} onClick={() => setCallDirection(id(), "callees")}>
                <PhoneOutgoing size={15} />
              </ToolButton>
            </>
          );
        }}
      </Show>
      <span class="hierarchy-sep" />
      <ToolButton
        title={tr("common.sortAlphabetically")}
        on={tab().sortAlphabetically}
        onClick={() => updateTab(id(), (t) => (t.sortAlphabetically = !t.sortAlphabetically))}
      >
        <ArrowDownAZ size={15} />
      </ToolButton>
      <button class="hierarchy-scope" title={tr("common.scope")} onClick={scopeMenu}>
        {tr("common.scopeLabel")} <span class="hierarchy-scope-value">{SCOPES.find((s) => s.id === tab().scope)?.label}</span>
        <ChevronDown size={12} />
      </button>
      <span class="hierarchy-sep" />
      <ToolButton
        title={tr("common.expandAll2")}
        onClick={() =>
          updateTab(id(), (t) => {
            if (t.kind === "call") {
              // Only what is loaded: expanding everything would ask for the whole call graph.
              t.expanded = [...new Set([...t.expanded, ...rowsOf(t).filter((r) => r.hasChildren && t.calls[r.key] !== undefined).map((r) => r.key)])];
            } else t.collapsed = [];
          })
        }
      >
        <ChevronsUpDown size={15} />
      </ToolButton>
      <ToolButton
        title={tr("common.collapseAll2")}
        onClick={() =>
          updateTab(id(), (t) => {
            if (t.kind === "call") t.expanded = [];
            else t.collapsed = allKeys(t);
          })
        }
      >
        <ChevronsDownUp size={15} />
      </ToolButton>
      <ToolButton title={tr("tools.hierarchy.autoscrollToSource")} on={autoscroll()} onClick={() => setAutoscroll(!autoscroll())}>
        <Crosshair size={15} />
      </ToolButton>
      <ToolButton title={tr("common.refresh")} onClick={() => void refreshTab(id())}>
        <RotateCw size={15} />
      </ToolButton>
    </div>
  );
}

function Row(props: { tab: HierarchyTab; row: HierarchyRow; onToggle: (row: HierarchyRow, open: boolean) => void }) {
  const r = () => props.row;
  const select = () => {
    updateTab(props.tab.id, (t) => (t.selected = r().key));
    if (autoscroll() && !r().status) openItem(r().item);
  };
  return (
    <div
      class="hierarchy-row"
      classList={{ selected: props.tab.selected === r().key, current: !!r().current, status: !!r().status, error: r().status === "error" }}
      style={{ "padding-left": `${8 + r().depth * 16}px` }}
      role="treeitem"
      aria-expanded={r().hasChildren ? r().expanded : undefined}
      data-key={r().key}
      title={r().item.path ? `${r().item.path}:${r().item.line + 1}` : undefined}
      onClick={select}
      onDblClick={() => !r().status && openItem(r().item)}
    >
      <Show when={!r().status} fallback={<span class="hierarchy-message">{r().message}</span>}>
        <span
          class="hierarchy-twisty"
          onClick={(e) => {
            e.stopPropagation();
            if (r().hasChildren) props.onToggle(r(), !r().expanded);
          }}
        >
          <Show when={r().hasChildren}>
            <Show when={r().expanded} fallback={<ChevronRight size={13} />}>
              <ChevronDown size={13} />
            </Show>
          </Show>
        </span>
        <Show when={r().mark}>{(m) => <MarkIcon mark={m()} />}</Show>
        <ItemIcon item={r().item} />
        <span class="hierarchy-name">{r().item.name}</span>
        <Show when={r().item.container}>
          <span class="hierarchy-container">{r().item.container}</span>
        </Show>
        <Show when={r().calls && r().calls!.length > 1}>
          <span class="hierarchy-count">{tr("tools.hierarchy.usages", { count: r().calls!.length })}</span>
        </Show>
        <Show when={r().recursive}>
          <span class="hierarchy-count" title={tr("tools.hierarchy.recursiveCall")}>
            <RotateCw size={11} />
          </span>
        </Show>
      </Show>
    </div>
  );
}

/** The call sites of the selected Call Hierarchy node ("Show usages"). */
function CallSites(props: { row: HierarchyRow | undefined }) {
  const calls = () => props.row?.calls ?? [];
  return (
    <div class="hierarchy-usages" aria-label={tr("tools.hierarchy.usages2")}>
      <Show
        when={calls().length}
        fallback={<div class="hierarchy-usages-empty muted">{tr("tools.hierarchy.selectACallerOrCallee")}</div>}
      >
        <For each={calls()}>
          {(c) => (
            <div
              class="hierarchy-usage"
              title={`${c.path}:${c.range.start.line + 1}`}
              onClick={() => openItem({ path: c.path, line: c.range.start.line, column: c.range.start.character })}
            >
              <FileIcon name={c.path.split("/").pop() ?? c.path} size={13} />
              <span class="hierarchy-usage-file">{c.path.split("/").pop()}</span>
              <span class="hierarchy-usage-line">{c.range.start.line + 1}</span>
              <span class="hierarchy-usage-code">{c.preview}</span>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}

function Legend() {
  const marks: MethodMark[] = ["defined", "shouldDefine", "inherited"];
  return (
    <div class="hierarchy-legend">
      <For each={marks}>
        {(m) => (
          <span class="hierarchy-legend-item">
            <MarkIcon mark={m} />
            {MARK_TITLES[m]}
          </span>
        )}
      </For>
    </div>
  );
}

function TabView(props: { tab: HierarchyTab }) {
  let tree!: HTMLDivElement;
  const rows = createMemo(() => rowsOf(props.tab));
  const selectedRow = () => rows().find((r) => r.key === props.tab.selected);

  const toggle = (row: HierarchyRow, open: boolean) => {
    const id = props.tab.id;
    if (props.tab.kind === "call") {
      updateTab(id, (t) => (t.expanded = toggleKey(t.expanded, row.key, open)));
      if (open) void loadCalls(id, row.key, row.item);
    } else {
      updateTab(id, (t) => (t.collapsed = toggleKey(t.collapsed, row.key, !open)));
    }
  };

  const onKeyDown = (e: KeyboardEvent) => {
    const list = rows().filter((r) => !r.status);
    const sel = selectedRow();
    if ((e.key === "Enter" || e.key === "F4") && sel) {
      e.preventDefault();
      openItem(sel.item);
      return;
    }
    const action = treeNavigate(
      list.map((r) => ({ path: r.key, depth: r.depth, isDir: r.hasChildren, expanded: r.expanded })),
      props.tab.selected,
      e.key,
    );
    if (action.type === "none") return;
    e.preventDefault();
    const row = list.find((r) => r.key === action.path);
    if (!row) return;
    if (action.type === "select") {
      updateTab(props.tab.id, (t) => (t.selected = row.key));
      tree.querySelector(`[data-key="${CSS.escape(row.key)}"]`)?.scrollIntoView({ block: "nearest" });
      if (autoscroll()) openItem(row.item);
    } else if (action.type === "expand") toggle(row, true);
    else if (action.type === "collapse") toggle(row, false);
  };

  return (
    <div class="hierarchy-window" classList={{ [`kind-${props.tab.kind}`]: true }}>
      <Toolbar tab={props.tab} />
      <div class="hierarchy-tree" role="tree" tabIndex={0} ref={tree} onKeyDown={onKeyDown}>
        <For each={rows()}>{(row) => <Row tab={props.tab} row={row} onToggle={toggle} />}</For>
      </div>
      <Show when={props.tab.kind === "call"}>
        <CallSites row={selectedRow()} />
      </Show>
      <Show when={props.tab.kind === "method"}>
        <Legend />
      </Show>
    </div>
  );
}

export function HierarchyToolWindow() {
  return (
    <Show
      when={activeHierarchyTab()}
      keyed
      fallback={
        <div class="hierarchy-empty">
          <Network size={28} class="muted" />
          <div>
            {tr("tools.hierarchy.typeHierarchyMethodHierarchyAnd", { value: shortcutLabel("TypeHierarchy"), value2: shortcutLabel("MethodHierarchy"), value3: shortcutLabel("CallHierarchy") })}
          </div>
        </div>
      }
    >
      {(t) => <TabView tab={t} />}
    </Show>
  );
}
