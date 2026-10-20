/** Lazy variables tree (Variables pane, Watches, Evaluate result). */
import { createEffect, createSignal, For, on, Show, type JSX } from "solid-js";
import { Braces, Brackets, ChevronDown, ChevronRight, Glasses, Variable, X } from "lucide-solid";
import { debugApi, type Property } from "./api";
import { formatValue, typeLabel, valueText } from "./format";
import { contextVersion, pausedSession, selectedFrame } from "./store";

function TypeIcon(props: { p: Property; watch?: boolean }) {
  if (props.watch) return <Glasses size={13} class="dbg-icon dbg-icon-watch" />;
  if (props.p.type === "object") return <Braces size={13} class="dbg-icon dbg-icon-object" />;
  if (props.p.type === "array") return <Brackets size={13} class="dbg-icon dbg-icon-array" />;
  return <Variable size={13} class="dbg-icon dbg-icon-scalar" />;
}

function ValueSpan(props: { p: Property }) {
  const cls = () => `dbg-value dbg-value-${props.p.type}`;
  return (
    <>
      <Show when={typeLabel(props.p)}>
        <span class="dbg-type">{typeLabel(props.p)} </span>
      </Show>
      <span class={cls()}>{valueText(props.p)}</span>
    </>
  );
}

export interface NodeProps {
  p: Property;
  depth: number;
  /** Stack depth the property belongs to. */
  frame?: number;
  /** Shown instead of the name (watch expressions). */
  label?: string;
  watch?: boolean;
  onRemove?: () => void;
  initiallyOpen?: boolean;
  selected?: string | null;
  onSelect?: (key: string) => void;
  /** Extra row content (e.g. error text). */
  children?: JSX.Element;
}

/** Load all children of `p` up to the page `p` describes. */
async function loadChildren(sessionId: string, p: Property, frame: number, page: number): Promise<Property> {
  return debugApi.property(sessionId, p.fullname, frame, 0, page);
}

export function VariableNode(props: NodeProps) {
  const [open, setOpen] = createSignal(!!props.initiallyOpen);
  const [children, setChildren] = createSignal<Property[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [nextPage, setNextPage] = createSignal(0);
  const key = () => `${props.label ?? ""}|${props.p.fullname}`;
  const frame = () => props.frame ?? selectedFrame()?.level ?? 0;
  const complete = (p: Property) => p.children.length > 0 && p.children.length >= Math.min(p.numChildren, p.pageSize || p.numChildren);

  const fetchPage = async (page: number, append: boolean) => {
    const s = pausedSession();
    if (!s) return;
    setLoading(true);
    setError(null);
    try {
      const full = await loadChildren(s.id, props.p, frame(), page);
      setChildren((prev) => (append ? [...prev, ...full.children] : full.children));
      setNextPage(page + 1);
    } catch (e) {
      setError(String(e).replace(/^Error: /, ""));
    } finally {
      setLoading(false);
    }
  };

  const ensureChildren = () => {
    if (props.p.children.length && complete(props.p)) {
      setChildren(props.p.children);
      setNextPage(1);
    } else void fetchPage(0, false);
  };

  // Reload expanded nodes after a step (values changed).
  createEffect(
    on(
      () => [contextVersion(), props.p] as const,
      () => {
        if (open()) ensureChildren();
      },
      { defer: true },
    ),
  );
  if (props.initiallyOpen && props.p.hasChildren) ensureChildren();

  const toggle = () => {
    if (!props.p.hasChildren) return;
    const next = !open();
    setOpen(next);
    if (next) ensureChildren();
  };
  const more = () => Math.max(0, props.p.numChildren - children().length);
  const indent = () => ({ "padding-left": `${8 + props.depth * 16}px` });

  return (
    <>
      <div
        class="tree-row dbg-row"
        classList={{ selected: props.selected === key() }}
        style={indent()}
        onClick={() => props.onSelect?.(key())}
        onDblClick={toggle}
        title={formatValue(props.p)}
        data-var={props.p.fullname}
      >
        <span class="tree-chevron" onClick={(e) => (e.stopPropagation(), toggle())}>
          <Show when={props.p.hasChildren}>{open() ? <ChevronDown size={13} /> : <ChevronRight size={13} />}</Show>
        </span>
        <TypeIcon p={props.p} watch={props.watch} />
        <span class="dbg-name" classList={{ "dbg-name-watch": props.watch }}>
          {props.label ?? props.p.name}
        </span>
        <Show when={props.children} fallback={<><span class="dbg-eq"> = </span><ValueSpan p={props.p} /></>}>
          {props.children}
        </Show>
        <Show when={props.onRemove}>
          <button class="icon-btn dbg-row-action" title="Remove Watch" onClick={(e) => (e.stopPropagation(), props.onRemove?.())}>
            <X size={12} />
          </button>
        </Show>
      </div>
      <Show when={open()}>
        <Show when={error()}>
          <div class="tree-row dbg-row dbg-error" style={{ "padding-left": `${8 + (props.depth + 1) * 16 + 18}px` }}>
            {error()}
          </div>
        </Show>
        <For each={children()}>
          {(c) => (
            <VariableNode p={c} depth={props.depth + 1} frame={props.frame} selected={props.selected} onSelect={props.onSelect} />
          )}
        </For>
        <Show when={loading()}>
          <div class="tree-row dbg-row muted" style={{ "padding-left": `${8 + (props.depth + 1) * 16 + 18}px` }}>
            Collecting data…
          </div>
        </Show>
        <Show when={!loading() && more() > 0 && children().length > 0}>
          <div
            class="tree-row dbg-row dbg-more"
            style={{ "padding-left": `${8 + (props.depth + 1) * 16 + 18}px` }}
            onClick={() => void fetchPage(nextPage(), true)}
          >
            {more()} more…
          </div>
        </Show>
      </Show>
    </>
  );
}
