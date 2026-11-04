/**
 * Alt+Insert "Generate" popup at the caret and the PHPStorm-style member chooser dialog
 * ("Choose Fields to Initialize by Constructor", "Select Methods to Override", ...).
 */
import { createMemo, createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { createStore } from "solid-js/store";
import type { EditorView } from "@codemirror/view";
import { ArrowDownAZ, X } from "lucide-solid";
import { phpApi, type GenerateKind, type GenerateTargets } from "./api";
import { applyEdits, viewFor } from "./editorViews";
import {
  chooserTitle,
  clampToViewport,
  fieldsFor,
  generateMenu,
  groupByOwner,
  methodsFor,
  needsChooser,
  showsFluentOption,
  type GenerateMenuItem,
} from "./generateModel";
import { SymbolIcon } from "./SymbolIcon";
import { generateItems } from "@/editor/generateItems";
import { t as tr } from "@/i18n";

interface Ctx {
  path: string;
  offset: number;
  targets: GenerateTargets;
}

interface State {
  popup: (Ctx & { x: number; y: number; items: GenerateMenuItem[]; active: number }) | null;
  chooser: (Ctx & { kind: GenerateKind; selected: string[]; fluent: boolean; focus: number; sorted: boolean }) | null;
  balloon: { x: number; y: number; text: string } | null;
}

const [state, setState] = createStore<State>({ popup: null, chooser: null, balloon: null });

export const generateState = state;

function caretCoords(view: EditorView, offset: number): { x: number; y: number } {
  const c = view.coordsAtPos(offset);
  return c ? { x: c.left, y: c.bottom + 2 } : { x: 200, y: 200 };
}

function showBalloon(x: number, y: number, text: string) {
  setState("balloon", { x, y, text });
  setTimeout(() => setState("balloon", (b) => (b?.text === text ? null : b)), 2500);
}

/** Open the Generate popup for the caret of `view` (file `path`). */
export async function openGeneratePopup(path: string, view: EditorView): Promise<void> {
  const offset = view.state.selection.main.head;
  const content = view.state.doc.toString();
  const targets = await phpApi.generateTargets(path, offset, content);
  const items = [...generateMenu(targets), ...extraItems(path, view)];
  const { x, y } = caretCoords(view, offset);
  setState({
    chooser: null,
    popup: { path, offset, targets, x, y, items, active: Math.max(0, items.findIndex((i) => i.enabled)) },
  });
}

/** Entries of other features (`registerGenerateItems`), after a separator. */
function extraItems(path: string, view: EditorView): GenerateMenuItem[] {
  return generateItems(path, view).map((g, i) => ({ kind: "extra", label: g.label, mnemonic: g.mnemonic, enabled: g.enabled, shortcut: g.shortcut, run: g.run, separator: i === 0 }));
}

/** Files without a Generate implementation: a popup with only the other features' entries. */
export function openGenerateItemsPopup(path: string, view: EditorView): boolean {
  const items = extraItems(path, view).map((i) => ({ ...i, separator: false }));
  if (!items.length) return false;
  const offset = view.state.selection.main.head;
  const { x, y } = caretCoords(view, offset);
  setState({ chooser: null, popup: { path, offset, targets: {} as GenerateTargets, x, y, items, active: Math.max(0, items.findIndex((i) => i.enabled)) } });
  return true;
}

/** Run an action directly (used by Ctrl+O / Ctrl+I style shortcuts and tests). */
export async function startGenerate(path: string, view: EditorView, kind: GenerateKind): Promise<void> {
  const offset = view.state.selection.main.head;
  const targets = await phpApi.generateTargets(path, offset, view.state.doc.toString());
  pick({ path, offset, targets }, kind);
}

function initialSelection(kind: GenerateKind, t: GenerateTargets): string[] {
  if (kind === "overrideMethods") return [];
  if (kind === "implementMethods") return t.implementMethods.map((m) => m.id);
  return fieldsFor(kind, t).map((f) => f.name);
}

function pick(ctx: Ctx, kind: GenerateKind) {
  setState("popup", null);
  if (!needsChooser(kind)) {
    void run({ ...ctx, kind, selected: [], fluent: false });
    return;
  }
  setState("chooser", { ...ctx, kind, selected: initialSelection(kind, ctx.targets), fluent: false, focus: 0, sorted: false });
}

async function run(c: Ctx & { kind: GenerateKind; selected: string[]; fluent: boolean }) {
  const view = viewFor(c.path);
  if (!view) return;
  const isMethods = c.kind === "overrideMethods" || c.kind === "implementMethods";
  try {
    const edits = await phpApi.generate(
      c.path,
      c.offset,
      c.kind,
      isMethods ? { methods: c.selected } : { fields: c.selected, fluent: c.fluent },
      view.state.doc.toString(),
    );
    setState("chooser", null);
    applyEdits(view, edits);
  } catch (e) {
    setState("chooser", null);
    const { x, y } = caretCoords(view, c.offset);
    showBalloon(x, y, e instanceof Error ? e.message : String(e));
    view.focus();
  }
}

function close() {
  const ctx = state.popup ?? state.chooser;
  setState({ popup: null, chooser: null });
  if (ctx) viewFor(ctx.path)?.focus();
}

function GeneratePopup() {
  let el!: HTMLDivElement;
  const p = () => state.popup!;
  const [pos, setPos] = createSignal({ x: p().x, y: p().y });
  onMount(() => {
    setPos(clampToViewport({ x: p().x, y: p().y }, el.getBoundingClientRect(), { width: innerWidth, height: innerHeight }));
    el.focus();
  });
  const move = (dir: 1 | -1) => {
    const items = p().items;
    let i = p().active;
    for (let n = 0; n < items.length; n++) {
      i = (i + dir + items.length) % items.length;
      if (items[i].enabled) break;
    }
    setState("popup", "active", i);
  };
  const choose = (item: GenerateMenuItem) => {
    if (!item.enabled) return;
    if (item.kind === "extra") {
      setState("popup", null);
      item.run?.();
      return;
    }
    const { path, offset, targets } = p();
    pick({ path, offset, targets }, item.kind);
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "ArrowDown") move(1);
    else if (e.key === "ArrowUp") move(-1);
    else if (e.key === "Enter") choose(p().items[p().active]);
    else if (e.key === "Escape") close();
    else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey) {
      const item = p().items.find((i) => i.enabled && i.label[i.mnemonic]?.toLowerCase() === e.key.toLowerCase());
      if (item) choose(item);
      else return;
    } else return;
    e.preventDefault();
    e.stopPropagation();
  };
  return (
    <div
      ref={el}
      class="php-generate-popup"
      role="menu"
      tabindex={-1}
      style={{ left: `${pos().x}px`, top: `${pos().y}px` }}
      onKeyDown={onKey}
      onFocusOut={(e) => !el.contains(e.relatedTarget as Node) && setTimeout(() => state.popup && !el.contains(document.activeElement) && close(), 0)}
    >
      <div class="php-popup-title">{tr("langs.php.generate")}</div>
      <For each={p().items}>
        {(item, i) => (
          <>
            <Show when={item.separator}>
              <div class="php-popup-sep" />
            </Show>
            <div
              class="php-popup-item"
              role="menuitem"
              classList={{ active: p().active === i(), disabled: !item.enabled }}
              aria-disabled={!item.enabled}
              onMouseEnter={() => item.enabled && setState("popup", "active", i())}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => choose(item)}
            >
              <span class="php-popup-label">
                {item.label.slice(0, item.mnemonic)}
                <u>{item.label[item.mnemonic]}</u>
                {item.label.slice(item.mnemonic + 1)}
              </span>
              <Show when={item.shortcut}>
                <span class="php-popup-shortcut">{item.shortcut}</span>
              </Show>
            </div>
          </>
        )}
      </For>
    </div>
  );
}

interface Row {
  id: string;
  /** Group header rows are not selectable. */
  header?: boolean;
  render: () => JSX.Element;
}

function ChooserDialog() {
  let listEl!: HTMLDivElement;
  const c = () => state.chooser!;
  const isMethods = () => c().kind === "overrideMethods" || c().kind === "implementMethods";

  const rows = createMemo<Row[]>(() => {
    const ch = c();
    if (isMethods()) {
      const out: Row[] = [];
      for (const g of groupByOwner(methodsFor(ch.kind, ch.targets))) {
        out.push({
          id: `#${g.owner}`,
          header: true,
          render: () => (
            <>
              <SymbolIcon kind={g.kind as "class"} />
              <span class="php-chooser-name">{g.owner.split("\\").pop()}</span>
              <span class="php-chooser-detail">{g.owner.includes("\\") ? g.owner.slice(0, g.owner.lastIndexOf("\\")) : ""}</span>
            </>
          ),
        });
        const methods = ch.sorted ? [...g.methods].sort((a, b) => a.name.localeCompare(b.name)) : g.methods;
        for (const m of methods)
          out.push({
            id: m.id,
            render: () => (
              <>
                <SymbolIcon kind="method" visibility={m.visibility} modifiers={{ isStatic: m.isStatic, isAbstract: ch.kind === "implementMethods" }} />
                <span class="php-chooser-name">{m.name}</span>
                <span class="php-chooser-detail">{m.signature.slice(m.name.length)}</span>
              </>
            ),
          });
      }
      return out;
    }
    const fields = fieldsFor(ch.kind, ch.targets);
    const sorted = ch.sorted ? [...fields].sort((a, b) => a.name.localeCompare(b.name)) : fields;
    return [
      {
        id: "#class",
        header: true,
        render: () => (
          <>
            <SymbolIcon kind={ch.targets.classKind ?? "class"} />
            <span class="php-chooser-name">{ch.targets.className}</span>
          </>
        ),
      },
      ...sorted.map((f) => ({
        id: f.name,
        render: () => (
          <>
            <SymbolIcon kind="property" visibility={f.visibility} modifiers={{ isStatic: f.isStatic, isReadonly: f.isReadonly }} />
            <span class="php-chooser-name">${f.name}</span>
            <Show when={f.typeHint}>
              <span class="php-chooser-detail">: {f.typeHint}</span>
            </Show>
          </>
        ),
      })),
    ];
  });

  const selectable = () => rows().filter((r) => !r.header);
  const isSelected = (id: string) => c().selected.includes(id);
  const toggle = (id: string) =>
    setState("chooser", "selected", (s) => (s.includes(id) ? s.filter((x) => x !== id) : [...s, id]));
  const selectAll = () => setState("chooser", "selected", selectable().map((r) => r.id));
  const selectNone = () => setState("chooser", "selected", []);
  const ok = () => {
    const ch = c();
    // Keep display order for the backend.
    const order = selectable().map((r) => r.id);
    void run({ ...ch, selected: order.filter((id) => ch.selected.includes(id)) });
  };
  const canOk = () => c().selected.length > 0 || c().kind === "constructor" || c().kind === "toString";

  const onKey = (e: KeyboardEvent) => {
    const list = selectable();
    if (e.key === "Escape") close();
    else if (e.key === "Enter") canOk() && ok();
    else if (e.key === "ArrowDown") setState("chooser", "focus", (f) => Math.min(list.length - 1, f + 1));
    else if (e.key === "ArrowUp") setState("chooser", "focus", (f) => Math.max(0, f - 1));
    else if (e.key === " " && list[c().focus]) toggle(list[c().focus].id);
    else if (e.key.toLowerCase() === "a" && (e.ctrlKey || e.metaKey)) selectAll();
    else return;
    e.preventDefault();
    e.stopPropagation();
  };

  onMount(() => listEl.focus());
  const keyListener = (e: KeyboardEvent) => e.key === "Escape" && close();
  document.addEventListener("keydown", keyListener);
  onCleanup(() => document.removeEventListener("keydown", keyListener));

  return (
    <div class="php-dialog-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div class="php-dialog" role="dialog" aria-label={chooserTitle(c().kind)} onKeyDown={onKey}>
        <header class="php-dialog-title">
          <span>{chooserTitle(c().kind)}</span>
          <button class="icon-btn" title={tr("common.close")} onClick={close}>
            <X size={14} />
          </button>
        </header>
        <div class="php-dialog-toolbar">
          <button
            class="icon-btn"
            classList={{ active: c().sorted }}
            title={tr("common.sortAlphabetically")}
            onClick={() => setState("chooser", "sorted", (s) => !s)}
          >
            <ArrowDownAZ size={15} />
          </button>
        </div>
        <div class="php-chooser-list" ref={listEl} tabindex={0} role="listbox" aria-multiselectable="true">
          <For each={rows()}>
            {(row) => {
              const index = () => selectable().findIndex((r) => r.id === row.id);
              return (
                <div
                  class="php-chooser-row"
                  classList={{
                    header: !!row.header,
                    selected: !row.header && isSelected(row.id),
                    focused: !row.header && index() === c().focus,
                  }}
                  role={row.header ? "presentation" : "option"}
                  aria-selected={!row.header && isSelected(row.id)}
                  onClick={() => {
                    if (row.header) return;
                    setState("chooser", "focus", index());
                    toggle(row.id);
                  }}
                >
                  <Show when={!row.header}>
                    <input type="checkbox" checked={isSelected(row.id)} tabindex={-1} onClick={(e) => e.preventDefault()} />
                  </Show>
                  {row.render()}
                </div>
              );
            }}
          </For>
          <Show when={selectable().length === 0}>
            <div class="php-chooser-empty">{tr("langs.php.nothingToChoose")}</div>
          </Show>
        </div>
        <Show when={showsFluentOption(c().kind)}>
          <label class="php-dialog-option">
            <input type="checkbox" checked={c().fluent} onChange={(e) => setState("chooser", "fluent", e.currentTarget.checked)} />
            {tr("langs.php.fluentSetters")}
          </label>
        </Show>
        <footer class="php-dialog-buttons">
          <button class="php-btn" onClick={selectAll}>
            {tr("langs.php.selectAll")}
          </button>
          <button class="php-btn" onClick={selectNone}>
            {tr("langs.php.selectNone")}
          </button>
          <span class="spacer" />
          <button class="php-btn" onClick={close}>
            {tr("common.cancel")}
          </button>
          <button class="php-btn primary" disabled={!canOk()} onClick={ok}>
            OK
          </button>
        </footer>
      </div>
    </div>
  );
}

export function GenerateOverlay() {
  return (
    <>
      <Show when={state.popup} keyed>
        <GeneratePopup />
      </Show>
      <Show when={state.chooser ? `${state.chooser.kind}:${state.chooser.offset}` : null} keyed>
        <ChooserDialog />
      </Show>
      <Show when={state.balloon}>
        {(b) => (
          <div class="php-balloon" style={{ left: `${b().x}px`, top: `${b().y}px` }}>
            {b().text}
          </div>
        )}
      </Show>
    </>
  );
}
