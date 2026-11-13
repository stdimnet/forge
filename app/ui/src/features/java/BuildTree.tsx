/**
 * The tree of the Gradle and Maven tool windows: nodes with icons, secondary text,
 * checkboxes (Maven profiles), expand / collapse (also all at once) and selection. The
 * windows build the nodes from the build model; this component only renders them.
 */
import { createSignal, For, Show, type Component, type JSX } from "solid-js";
import { ChevronDown, ChevronRight } from "lucide-solid";

export interface TreeNode {
  /** Unique in the tree (the expansion state is kept by it). */
  id: string;
  label: string;
  /** Muted text after the label (versions, scopes, descriptions). */
  secondary?: string;
  icon: Component<{ size?: number; class?: string }>;
  iconClass?: string;
  title?: string;
  /** Text color: `danger` (unresolved), `muted` (omitted). */
  tone?: "danger" | "muted";
  checkbox?: { checked: boolean; onToggle: () => void };
  children?: TreeNode[];
  /** Double click / Enter: run the task, open the file. */
  onActivate?: () => void;
  /** Data attributes for tests and screenshots. */
  data?: Record<string, string>;
}

export interface TreeState {
  expanded: () => Set<string>;
  setExpanded: (s: Set<string>) => void;
  selected: () => string | null;
  setSelected: (id: string | null) => void;
}

export function createTreeState(initial: string[] = []): TreeState {
  const [expanded, setExpanded] = createSignal<Set<string>>(new Set(initial));
  const [selected, setSelected] = createSignal<string | null>(null);
  return { expanded, setExpanded, selected, setSelected };
}

/** Every id with children (Expand All). */
export function branchIds(nodes: TreeNode[], depth = 99, out: string[] = []): string[] {
  for (const n of nodes) {
    if (n.children?.length && depth > 0) {
      out.push(n.id);
      branchIds(n.children, depth - 1, out);
    }
  }
  return out;
}

export function findNode(nodes: TreeNode[], id: string | null): TreeNode | undefined {
  if (!id) return undefined;
  for (const n of nodes) {
    if (n.id === id) return n;
    const inner = n.children && findNode(n.children, id);
    if (inner) return inner;
  }
  return undefined;
}

export function BuildTree(props: { nodes: TreeNode[]; state: TreeState; empty?: JSX.Element }) {
  const toggle = (id: string) => {
    const next = new Set(props.state.expanded());
    if (next.has(id)) next.delete(id);
    else next.add(id);
    props.state.setExpanded(next);
  };
  const Row = (p: { node: TreeNode; depth: number }) => {
    const n = p.node;
    const branch = () => !!n.children?.length;
    const open = () => props.state.expanded().has(n.id);
    return (
      <>
        <div
          class="tree-row java-bt-row"
          classList={{ selected: props.state.selected() === n.id, [`java-bt-${n.tone}`]: !!n.tone }}
          style={{ "padding-left": `${6 + p.depth * 16}px` }}
          title={n.title ?? n.label}
          onClick={() => props.state.setSelected(n.id)}
          onDblClick={() => (n.onActivate ? n.onActivate() : branch() && toggle(n.id))}
          {...Object.fromEntries(Object.entries(n.data ?? {}).map(([k, v]) => [`data-${k}`, v]))}
        >
          <span class="java-bt-twisty" onClick={(e) => (e.stopPropagation(), branch() && toggle(n.id))}>
            <Show when={branch()}>{open() ? <ChevronDown size={13} /> : <ChevronRight size={13} />}</Show>
          </span>
          <Show when={n.checkbox}>
            {(c) => (
              <input
                type="checkbox"
                class="java-bt-check"
                checked={c().checked}
                onClick={(e) => e.stopPropagation()}
                onChange={() => c().onToggle()}
              />
            )}
          </Show>
          <n.icon size={14} class={n.iconClass ?? "muted"} />
          <span class="java-bt-label">{n.label}</span>
          <Show when={n.secondary}>
            <span class="java-bt-secondary">{n.secondary}</span>
          </Show>
        </div>
        <Show when={branch() && open()}>
          <For each={n.children}>{(c) => <Row node={c} depth={p.depth + 1} />}</For>
        </Show>
      </>
    );
  };
  return (
    <div
      class="java-build-tree"
      tabIndex={0}
      onKeyDown={(e) => {
        const n = findNode(props.nodes, props.state.selected());
        if (!n) return;
        if (e.key === "Enter" && n.onActivate) n.onActivate();
        else if (e.key === "ArrowRight" && n.children?.length && !props.state.expanded().has(n.id)) toggle(n.id);
        else if (e.key === "ArrowLeft" && props.state.expanded().has(n.id)) toggle(n.id);
        else return;
        e.preventDefault();
      }}
    >
      <Show when={props.nodes.length} fallback={props.empty}>
        <For each={props.nodes}>{(n) => <Row node={n} depth={0} />}</For>
      </Show>
    </div>
  );
}
