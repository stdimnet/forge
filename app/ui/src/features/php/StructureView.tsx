/** Structure tool window (Alt+7): members of the active PHP file's classes. */
import { createEffect, createSignal, For, on, onCleanup, Show } from "solid-js";
import { ArrowDownAZ, ChevronDown, ChevronRight, Layers2 } from "lucide-solid";
import { activeTab } from "@/state/editor";
import { phpApi, type FileStructure, type StructureNode } from "./api";
import { revealSymbol } from "./editorViews";
import { SymbolIcon } from "./SymbolIcon";

const [sortAlpha, setSortAlpha] = createSignal(false);
const [showInherited, setShowInherited] = createSignal(false);
const [structure, setStructure] = createSignal<FileStructure | null>(null);
const [selected, setSelected] = createSignal<string | null>(null);
const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());

export { structure };

/** Order children: alphabetically when toggled (PHPStorm keeps kinds grouped). */
export function orderNodes(nodes: StructureNode[], alpha: boolean): StructureNode[] {
  if (!alpha) return nodes;
  const rank = (n: StructureNode) =>
    ({ constant: 0, enumCase: 0, property: 1, method: 2 } as Record<string, number>)[n.kind] ?? 3;
  return [...nodes].sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
}

let timer: ReturnType<typeof setTimeout> | undefined;
function refresh() {
  const tab = activeTab();
  if (!tab || !tab.path.endsWith(".php")) {
    setStructure(null);
    return;
  }
  const { path, content } = tab;
  clearTimeout(timer);
  timer = setTimeout(async () => {
    try {
      setStructure(await phpApi.fileStructure(path, content, showInherited()));
    } catch {
      setStructure(null);
    }
  }, 150);
}

export function StructureActions() {
  return (
    <>
      <button
        class="icon-btn"
        classList={{ active: sortAlpha() }}
        title="Sort Alphabetically"
        aria-pressed={sortAlpha()}
        onClick={() => setSortAlpha((v) => !v)}
      >
        <ArrowDownAZ size={15} />
      </button>
      <button
        class="icon-btn"
        classList={{ active: showInherited() }}
        title="Show Inherited"
        aria-pressed={showInherited()}
        onClick={() => setShowInherited((v) => !v)}
      >
        <Layers2 size={15} />
      </button>
    </>
  );
}

function NodeRow(props: { node: StructureNode; depth: number; parent: string; file: string }) {
  const key = () => `${props.parent}/${props.node.name}:${props.node.kind}`;
  const kids = () => (props.node.children ?? []).filter((c) => showInherited() || !c.inheritedFrom);
  const open = () => !collapsed().has(key());
  const navigate = () => {
    setSelected(key());
    const path = props.node.path ?? props.file;
    void revealSymbol(path, props.node.path ? { line: props.node.line, name: props.node.name } : { offset: props.node.offset, line: props.node.line, name: props.node.name });
  };
  return (
    <>
      <div
        class="tree-row php-structure-row"
        classList={{ selected: selected() === key(), inherited: !!props.node.inheritedFrom, deprecated: !!props.node.deprecated }}
        style={{ "padding-left": `${8 + props.depth * 16}px` }}
        title={props.node.inheritedFrom ? `Inherited from ${props.node.inheritedFrom}` : undefined}
        onClick={navigate}
      >
        <span
          class="tree-chevron"
          onClick={(e) => {
            if (!kids().length) return;
            e.stopPropagation();
            setCollapsed((s) => {
              const n = new Set(s);
              n.has(key()) ? n.delete(key()) : n.add(key());
              return n;
            });
          }}
        >
          <Show when={kids().length}>{open() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</Show>
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
  onCleanup(() => clearTimeout(timer));
  return (
    <div class="project-tree php-structure">
      <Show
        when={structure()}
        fallback={<div class="placeholder-panel">{activeTab() ? "No structure for this file" : "No file is open"}</div>}
      >
        {(s) => (
          <For each={orderNodes(s().nodes, sortAlpha())}>
            {(n) => <NodeRow node={n} depth={0} parent="" file={s().path} />}
          </For>
        )}
      </Show>
    </div>
  );
}
