/**
 * Structure tool window (Alt+7): members of the active PHP file's classes. Other language
 * packs (LuaForge) contribute the structure of their files with `registerStructureProvider`.
 * Options (⋮): Autoscroll to Source (a click navigates) and Autoscroll from Source (the node
 * at the caret is selected; picked in Rust like the File Structure popup's caret node).
 */
import { t } from "@/i18n";
import { navigate as navigateTree } from "@/features/project/treeNavigation";
import { createEffect, createSignal, For, on, onCleanup, Show } from "solid-js";
import { ArrowDownAZ, EllipsisVertical, Layers2 } from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { TreeArrow } from "@/shell/TreeArrow";
import { activeTab } from "@/state/editor";
import { languageIdFor } from "@/editor/languages";
import { fileStructureView, groupByRegions, scopesAt, structureOf, structureOfText } from "@/features/codeStructure/model";
import { activeView } from "@/features/lsp/navigation";
import { getSetting, saveSetting } from "@/state/settings";
import { showResolvedMenu, type ResolvedEntry } from "@/features/contextMenu/state";
import { phpApi, type FileStructure, type StructureNode } from "./api";
import { revealSymbol } from "./editorViews";
import { SymbolIcon } from "./SymbolIcon";

const [sortAlpha, setSortAlpha] = createSignal(false);
const [showInherited, setShowInherited] = createSignal(false);
const [structure, setStructure] = createSignal<FileStructure | null>(null);
const [selected, setSelected] = createSignal<string | null>(null);
const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());

export { structure };

/** The structure of another language's files, mapped to PHP's node model. */
export interface StructureProvider {
  appliesTo: (path: string) => boolean;
  load: (path: string, content: string) => Promise<FileStructure | null>;
  /** Navigate to a node (defaults to the PHP editor tracking). */
  reveal?: (path: string, node: StructureNode) => void;
}

const providers: StructureProvider[] = [];

export function registerStructureProvider(provider: StructureProvider): void {
  providers.push(provider);
}

const providerFor = (path: string) => providers.find((p) => p.appliesTo(path));

/** Order children: alphabetically when toggled (PHPStorm keeps kinds grouped). */
export function orderNodes(nodes: StructureNode[], alpha: boolean): StructureNode[] {
  if (!alpha) return nodes;
  const rank = (n: StructureNode) =>
    ({ constant: 0, enumCase: 0, property: 1, method: 2 } as Record<string, number>)[n.kind] ?? 3;
  return [...nodes].sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
}

/** Whether a file has a structure (a provider or the PHP index). */
export const hasFileStructure = (path: string): boolean => !!providerFor(path) || path.endsWith(".php");

/** Whether "Show Inherited" applies (PHP classes: members from parents via the PHP index). */
export const supportsInherited = (path: string): boolean => !providerFor(path) && path.endsWith(".php");

/**
 * The structure of a file from its provider (or the PHP index), with custom folding regions
 * grouping the members inside them like PHPStorm. Shared by the Structure tool window, the
 * File Structure popup (Ctrl+F12) and the Navigation Bar.
 */
export async function loadFileStructure(path: string, content: string, inherited = false): Promise<FileStructure | null> {
  const provider = providerFor(path);
  if (!provider && !path.endsWith(".php")) return null;
  const s = provider ? await provider.load(path, content) : await phpApi.fileStructure(path, content, inherited);
  const regions = structureOfText(content, languageIdFor(path)).regions;
  return s && { ...s, nodes: groupByRegions(s.nodes, regions) };
}

/** Move to a node of `file`'s structure (inherited members open their own file). */
export function revealStructureNode(file: string, node: StructureNode): void {
  const path = node.path ?? file;
  const custom = providerFor(path)?.reveal;
  if (custom) return custom(path, node);
  void revealSymbol(path, node.path ? { line: node.line, name: node.name } : { offset: node.offset, line: node.line, name: node.name });
}

let timer: ReturnType<typeof setTimeout> | undefined;
function refresh() {
  const tab = activeTab();
  if (!tab || !hasFileStructure(tab.path)) {
    setStructure(null);
    return;
  }
  const { path, content } = tab;
  clearTimeout(timer);
  timer = setTimeout(async () => {
    try {
      setStructure(await loadFileStructure(path, content, showInherited()));
    } catch {
      setStructure(null);
    }
  }, 150);
}

const TO_SOURCE = "structureView.autoscrollToSource";
const FROM_SOURCE = "structureView.autoscrollFromSource";
const nodeKey = (parent: string, node: StructureNode) => `${parent}/${node.name}:${node.kind}`;

/**
 * The index path (`"0/2"`: the third child of the first node) of the node at the caret of
 * the active editor, if it shows `s`'s file. Picked in Rust like the File Structure popup's
 * caret node; also used by the Project view's Show Members.
 */
export function caretNodeId(s: FileStructure, inherited = false): string | null {
  const a = activeView();
  if (!a || a.path !== s.path) return null;
  const { state } = a.view;
  const head = state.selection.main.head;
  const line = state.doc.lineAt(head);
  const scopes = scopesAt(structureOf(state.doc, languageIdFor(a.path)), line.number - 1, head - line.from);
  const view = fileStructureView({
    nodes: s.nodes,
    query: "",
    sortAlpha: false,
    showFields: true,
    showInherited: inherited,
    showLambdas: true,
    caretLine: line.number - 1,
    enclosing: scopes.map((sc) => ({ line: sc.line, openLine: sc.openLine })),
  });
  const row = view.selected == null ? undefined : view.rows[view.selected];
  return row?.id ?? null;
}

/** The keys from the root to the node at the caret of the active editor, if any. */
function caretPath(s: FileStructure): string[] {
  const id = caretNodeId(s, showInherited());
  if (id == null) return [];
  const keys: string[] = [];
  let nodes = s.nodes;
  let parent = "";
  for (const i of id.split("/").map(Number)) {
    const node = nodes[i];
    if (!node) break;
    parent = nodeKey(parent, node);
    keys.push(parent);
    nodes = node.children ?? [];
  }
  return keys;
}

/** Select the node at the caret, expanding its parents (Autoscroll from Source, Select In… › Structure). */
export function selectStructureAtCaret(): void {
  const s = structure();
  const keys = s ? caretPath(s) : [];
  const key = keys[keys.length - 1];
  if (!key) return;
  setCollapsed((c) => (keys.some((k) => c.has(k)) ? new Set([...c].filter((k) => !keys.includes(k))) : c));
  setSelected(key);
  requestAnimationFrame(() =>
    document.querySelector(`.php-structure .tree-row[data-key="${CSS.escape(key)}"]`)?.scrollIntoView({ block: "nearest" }),
  );
}

export function StructureActions() {
  const options = (e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const toggle = (key: string, label: string): ResolvedEntry => {
      const value = getSetting<boolean>(key, true);
      return { kind: "action", label, checkable: true, checked: value, run: () => void saveSetting(key, !value) };
    };
    showResolvedMenu([toggle(TO_SOURCE, "Autoscroll to Source"), toggle(FROM_SOURCE, "Autoscroll from Source")], r.left, r.bottom + 2);
  };
  return (
    <>
      <button
        class="icon-btn"
        classList={{ active: sortAlpha() }}
        title={t("common.sortAlphabetically")}
        aria-pressed={sortAlpha()}
        onClick={() => setSortAlpha((v) => !v)}
      >
        <ArrowDownAZ size={15} />
      </button>
      <button
        class="icon-btn"
        classList={{ active: showInherited() }}
        title={t("langs.php.showInherited")}
        aria-pressed={showInherited()}
        onClick={() => setShowInherited((v) => !v)}
      >
        <Layers2 size={15} />
      </button>
      <button class="icon-btn" title={t("langs.php.options")} aria-label={t("langs.php.options")} onClick={options}>
        <EllipsisVertical size={15} />
      </button>
    </>
  );
}

/** Row key → jump to its source (Enter / F4 in the tree). */
const navigators = new Map<string, () => void>();
/** Row key → element id (aria-activedescendant). */
const rowIds = new Map<string, string>();
const rowId = (key: string): string => {
  let id = rowIds.get(key);
  if (!id) rowIds.set(key, (id = `structure-row-${rowIds.size + 1}`));
  return id;
};

function toggleCollapsed(key: string): void {
  setCollapsed((s) => {
    const n = new Set(s);
    if (n.has(key)) n.delete(key);
    else n.add(key);
    return n;
  });
}

function NodeRow(props: { node: StructureNode; depth: number; parent: string; file: string }) {
  const key = () => nodeKey(props.parent, props.node);
  const kids = () => (props.node.children ?? []).filter((c) => showInherited() || !c.inheritedFrom);
  const open = () => !collapsed().has(key());
  const navigate = () => {
    setSelected(key());
    revealStructureNode(props.file, props.node);
  };
  createEffect(() => {
    const k = key();
    navigators.set(k, navigate);
    onCleanup(() => navigators.get(k) === navigate && navigators.delete(k));
  });
  const toSource = () => getSetting<boolean>(TO_SOURCE, true);
  return (
    <>
      <div
        class="tree-row php-structure-row"
        classList={{ selected: selected() === key(), inherited: !!props.node.inheritedFrom, deprecated: !!props.node.deprecated }}
        style={{ "padding-left": `${8 + props.depth * 16}px` }}
        data-key={key()}
        data-depth={props.depth}
        data-dir={kids().length ? "" : undefined}
        data-expanded={kids().length && open() ? "" : undefined}
        id={rowId(key())}
        role="treeitem"
        aria-level={props.depth + 1}
        aria-selected={selected() === key()}
        aria-expanded={kids().length ? open() : undefined}
        title={props.node.inheritedFrom ? t("langs.php.inheritedFrom", { inheritedFrom: props.node.inheritedFrom }) : undefined}
        onClick={() => (toSource() ? navigate() : setSelected(key()))}
        onDblClick={() => !toSource() && navigate()}
      >
        <span
          class="tree-chevron"
          onClick={(e) => {
            if (!kids().length) return;
            e.stopPropagation();
            toggleCollapsed(key());
          }}
        >
          <Show when={kids().length}>
            <TreeArrow open={open()} />
          </Show>
        </span>
        <SymbolIcon kind={props.node.kind} visibility={props.node.visibility} modifiers={props.node.modifiers} />
        <span class="tree-label php-structure-name">{props.node.name}</span>
        <span class="php-structure-detail">{props.node.detail}</span>
        <Show when={props.node.inheritedFrom}>
          <span class="php-structure-owner">↑{props.node.inheritedFrom}</span>
        </Show>
      </div>
      <Show when={open()}>
        <For each={orderNodes(kids(), sortAlpha())}>
          {(c) => <NodeRow node={c} depth={props.depth + 1} parent={key()} file={props.file} />}
        </For>
      </Show>
    </>
  );
}

export function StructureView() {
  createEffect(on([() => activeTab()?.path, () => activeTab()?.content, showInherited], refresh));
  // Autoscroll from Source: follow the caret.
  createEffect(
    on([structure, () => activeTab()?.cursor.line, () => getSetting<boolean>(FROM_SOURCE, true)], ([, , on]) => {
      if (on) selectStructureAtCaret();
    }),
  );
  onCleanup(() => clearTimeout(timer));
  // The tree keys of the Project view: arrows, Home/End, Enter/F4 to jump to the source.
  const onKeyDown = (e: KeyboardEvent) => {
    const root = e.currentTarget as HTMLElement;
    const rows = [...root.querySelectorAll<HTMLElement>(".php-structure-row[data-key]")].map((r) => ({
      path: r.dataset.key!,
      isDir: r.dataset.dir !== undefined,
      expanded: r.dataset.expanded !== undefined,
      depth: Number(r.dataset.depth ?? 0),
    }));
    const action = navigateTree(rows, selected(), e.key === "F4" ? "Enter" : e.key);
    if (action.type === "none") return;
    e.preventDefault();
    e.stopPropagation();
    if (action.type === "select") {
      setSelected(action.path);
      if (getSetting<boolean>(TO_SOURCE, true)) navigators.get(action.path)?.();
      document.getElementById(rowId(action.path))?.scrollIntoView?.({ block: "nearest" });
    } else if (action.type === "open") navigators.get(action.path)?.();
    else toggleCollapsed(action.path);
  };
  return (
    <div
      class="project-tree php-structure"
      tabIndex={0}
      role={structure() ? "tree" : undefined}
      aria-label={t("toolwindow.structure.title")}
      aria-activedescendant={selected() ? rowId(selected()!) : undefined}
      onKeyDown={onKeyDown}
    >
      <Show
        when={structure()}
        fallback={<div class="placeholder-panel">{activeTab() ? t("langs.php.noStructureForThisFile") : t("langs.php.noFileIsOpen")}</div>}
      >
        {(s) => (
          <>
            {/* The file the structure belongs to, as PHPStorm's root node. */}
            <div class="php-structure-file" title={s().path}>
              <FileIcon name={s().path.split("/").pop() ?? s().path} path={s().path} size={15} />
              <span>{s().path.split("/").pop()}</span>
            </div>
            <For each={orderNodes(s().nodes, sortAlpha())}>
              {(n) => <NodeRow node={n} depth={0} parent="" file={s().path} />}
            </For>
          </>
        )}
      </Show>
    </div>
  );
}
