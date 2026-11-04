/** Lazy variables tree (Variables pane, Watches, Evaluate result). */
import { createEffect, createSignal, For, on, onMount, Show, type JSX } from "solid-js";
import { Binary, Braces, Brackets, ChevronDown, ChevronRight, Copy, Eye, Glasses, Layers, Pencil, ScanEye, Variable, X } from "lucide-solid";
import { shortcutLabel } from "@/keymap";
import { openMenu } from "@/features/run/PopupMenu";
import { debugApi, type Property } from "./api";
import { formatValue, phpLiteral, typeLabel, valueText } from "./format";
import { setEvaluateRequest } from "./overlays";
import { openMemoryView } from "./nativeViews";
import {
  addWatch,
  addWatchpoint,
  contextVersion,
  editingVariable,
  pausedSession,
  selectedFrame,
  selectedVariable,
  setEditingVariable,
  setSelectedVariable,
  setVariableValue,
  sessionFeatures,
  type SelectedVariable,
} from "./store";
import { t } from "@/i18n";

function TypeIcon(props: { p: Property; watch?: boolean }) {
  if (props.watch) return <Glasses size={13} class="dbg-icon dbg-icon-watch" />;
  if (props.p.type === "scope") return <Layers size={13} class="dbg-icon dbg-icon-scope" />;
  if (props.p.type === "dap" && props.p.hasChildren) return <Braces size={13} class="dbg-icon dbg-icon-object" />;
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
  /** Rows of the Variables tree: selection, context menu and Set Value (F2). */
  selectable?: boolean;
  /** Extra row content (e.g. error text). */
  children?: JSX.Element;
}

/** Copy Value: scalars as shown, arrays and objects as `print_r` output. */
export async function copyValue(v: SelectedVariable): Promise<void> {
  const p = v.property;
  let text = p.type === "string" ? (p.value ?? "") : valueText(p);
  const s = pausedSession();
  if (s && !s.adapter && (p.type === "array" || p.type === "object")) {
    const printed = await debugApi.eval(s.id, `print_r(${v.watch ?? p.fullname}, true)`, selectedFrame()?.level ?? 0).catch(() => null);
    if (printed?.value != null) text = printed.value;
  }
  await navigator.clipboard?.writeText(text).catch(() => undefined);
}

/** Set Value (F2) on the selected row. */
export function editSelectedValue(): void {
  const v = selectedVariable();
  if (v && pausedSession()) setEditingVariable(v.key);
}

function openVariableMenu(e: MouseEvent, v: SelectedVariable, onRemove?: () => void): void {
  e.preventDefault();
  const expr = v.watch ?? v.property.fullname;
  const features = sessionFeatures();
  const p = v.property;
  const native = [
    ...(features?.dataBreakpoints && p.container != null && !v.watch
      ? [{ label: t("debug.variablesTree.addWatchpoint"), icon: ScanEye, disabled: !pausedSession(), run: () => void addWatchpoint(p) }]
      : []),
    ...(features?.readMemory && p.memoryReference
      ? [{ label: t("debug.variablesTree.viewMemory"), icon: Binary, run: () => openMemoryView(p.memoryReference!) }]
      : []),
  ];
  openMenu({
    x: e.clientX,
    y: e.clientY,
    items: [
      { label: t("debug.variablesTree.setValue"), icon: Pencil, shortcut: shortcutLabel("XDebugger.SetValue"), disabled: !pausedSession(), run: () => setEditingVariable(v.key) },
      { label: t("debug.variablesTree.copyValue"), icon: Copy, shortcut: shortcutLabel("XDebugger.CopyValue"), run: () => void copyValue(v) },
      { label: t("debug.variablesTree.copyName"), run: () => void navigator.clipboard?.writeText(expr).catch(() => undefined) },
      { separator: true },
      { label: t("debug.variablesTree.evaluateExpression"), shortcut: shortcutLabel("EvaluateExpression"), run: () => setEvaluateRequest({ expression: expr }) },
      { label: t("debug.variablesTree.addToWatches"), icon: Eye, disabled: !!v.watch, run: () => addWatch(expr) },
      ...(native.length ? [{ separator: true }, ...native] : []),
      ...(onRemove ? [{ separator: true }, { label: t("debug.variablesTree.removeWatch"), icon: X, run: onRemove }] : []),
    ],
  });
}

/** Inline editor of Set Value: Enter assigns the PHP expression, Escape cancels. */
function ValueEditor(props: { p: Property; expression: string }) {
  let input!: HTMLInputElement;
  const [text, setText] = createSignal(phpLiteral(props.p));
  onMount(() => {
    input.focus();
    input.select();
  });
  const done = () => setEditingVariable(null);
  return (
    <input
      ref={input}
      class="dbg-input dbg-mono dbg-value-edit"
      value={text()}
      spellcheck={false}
      onInput={(e) => setText(e.currentTarget.value)}
      onClick={(e) => e.stopPropagation()}
      onBlur={done}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          done();
        } else if (e.key === "Enter") {
          e.preventDefault();
          const value = text().trim();
          if (value) void setVariableValue(props.expression, value, props.p).then((ok) => ok && done());
        }
      }}
    />
  );
}

/** Load all children of `p` up to the page `p` describes (DAP: by its reference). */
async function loadChildren(sessionId: string, p: Property, frame: number, page: number): Promise<Property> {
  return debugApi.property(sessionId, p.fullname, frame, 0, page, p.reference);
}

export function VariableNode(props: NodeProps) {
  const [open, setOpen] = createSignal(!!props.initiallyOpen);
  const [children, setChildren] = createSignal<Property[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [nextPage, setNextPage] = createSignal(0);
  const key = () => `${props.label ?? ""}|${props.p.fullname || `#${props.p.name}`}`;
  const isSelected = () => (props.selectable ? selectedVariable()?.key === key() : props.selected === key());
  const editing = () => !!props.selectable && editingVariable() === key();
  const asSelection = (): SelectedVariable => ({ key: key(), property: props.p, watch: props.watch ? props.label : undefined });
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
        classList={{ selected: isSelected() }}
        style={indent()}
        onClick={() => (props.selectable ? setSelectedVariable(asSelection()) : props.onSelect?.(key()))}
        onDblClick={toggle}
        onContextMenu={(e) => {
          if (!props.selectable) return;
          setSelectedVariable(asSelection());
          openVariableMenu(e, asSelection(), props.onRemove);
        }}
        title={formatValue(props.p)}
        data-var={props.p.fullname || props.p.name}
      >
        <span class="tree-chevron" onClick={(e) => (e.stopPropagation(), toggle())}>
          <Show when={props.p.hasChildren}>{open() ? <ChevronDown size={13} /> : <ChevronRight size={13} />}</Show>
        </span>
        <TypeIcon p={props.p} watch={props.watch} />
        <span class="dbg-name" classList={{ "dbg-name-watch": props.watch }}>
          {props.label ?? props.p.name}
        </span>
        <Show
          when={!editing()}
          fallback={
            <>
              <span class="dbg-eq"> = </span>
              <ValueEditor p={props.p} expression={props.watch ? (props.label ?? props.p.fullname) : props.p.fullname} />
            </>
          }
        >
          <Show
            when={props.children}
            fallback={
              <Show when={props.p.type !== "scope"}>
                <span class="dbg-eq"> = </span>
                <ValueSpan p={props.p} />
              </Show>
            }
          >
            {props.children}
          </Show>
        </Show>
        <Show when={props.onRemove}>
          <button class="icon-btn dbg-row-action" title={t("debug.variablesTree.removeWatch")} onClick={(e) => (e.stopPropagation(), props.onRemove?.())}>
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
            <VariableNode
              p={c}
              depth={props.depth + 1}
              frame={props.frame}
              selected={props.selected}
              onSelect={props.onSelect}
              selectable={props.selectable}
            />
          )}
        </For>
        <Show when={loading()}>
          <div class="tree-row dbg-row muted" style={{ "padding-left": `${8 + (props.depth + 1) * 16 + 18}px` }}>
            {t("debug.variablesTree.collectingData")}
          </div>
        </Show>
        <Show when={!loading() && more() > 0 && children().length > 0}>
          <div
            class="tree-row dbg-row dbg-more"
            style={{ "padding-left": `${8 + (props.depth + 1) * 16 + 18}px` }}
            onClick={() => void fetchPage(nextPage(), true)}
          >
            {t("tools.debug.moreEllipsis", { more: more() })}
          </div>
        </Show>
      </Show>
    </>
  );
}
