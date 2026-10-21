/**
 * The caret popups of live templates: Insert Live Template (Ctrl+J), Surround With
 * (Ctrl+Alt+T), Surround with Live Template (Ctrl+Alt+J) and Unwrap/Remove
 * (Ctrl+Shift+Delete). The popup owns the keyboard while it is open: arrows, Enter, Escape,
 * mnemonics and, in filterable lists, typing to narrow the list (PHPStorm's speed search).
 */
import { createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { createStore } from "solid-js/store";
import type { EditorView } from "@codemirror/view";

export interface PopupItem {
  id: string;
  label: string;
  detail?: string;
  /** Key that picks the item, shown before the label (Surround With). */
  mnemonic?: string;
  /** Title of a separator shown before the item ("Live templates"). */
  section?: string;
  run: () => void;
  /** Called while the item is selected (Unwrap/Remove highlights the statement). */
  preview?: () => void;
}

interface PopupState {
  title: string;
  items: PopupItem[];
  x: number;
  y: number;
  /** Narrow the list by typing (Insert Live Template). */
  filterable: boolean;
  filter: string;
  view: EditorView;
  onClose?: () => void;
}

const [state, setState] = createStore<{ popup: PopupState | null }>({ popup: null });

export const popupOpen = () => !!state.popup;

/** Show a popup below the caret of `view`. */
export function openPopup(view: EditorView, opts: { title: string; items: PopupItem[]; filterable?: boolean; filter?: string; onClose?: () => void }): void {
  const head = view.state.selection.main.head;
  const c = view.coordsAtPos(head) ?? view.coordsAtPos(view.state.selection.main.from);
  setState("popup", {
    title: opts.title,
    items: opts.items,
    x: c ? c.left : 200,
    y: c ? c.bottom + 2 : 200,
    filterable: !!opts.filterable,
    filter: opts.filter ?? "",
    view,
    onClose: opts.onClose,
  });
}

export function closePopup(focusEditor = true): void {
  const p = state.popup;
  if (!p) return;
  setState("popup", null);
  p.onClose?.();
  if (focusEditor) p.view.focus();
}

/** Items matching the speed search: abbreviation prefix first, then anywhere in the text. */
export function filterItems(items: PopupItem[], filter: string): PopupItem[] {
  const f = filter.toLowerCase();
  if (!f) return items;
  const starts = items.filter((i) => i.label.toLowerCase().startsWith(f));
  const contains = items.filter((i) => !starts.includes(i) && `${i.label} ${i.detail ?? ""}`.toLowerCase().includes(f));
  return [...starts, ...contains];
}

function TemplatePopup(props: { popup: PopupState }) {
  let el!: HTMLDivElement;
  let list!: HTMLDivElement;
  const visible = createMemo(() => (props.popup.filterable ? filterItems(props.popup.items, props.popup.filter) : props.popup.items));
  const [active, setActive] = createSignal(0);
  const [pos, setPos] = createSignal({ x: props.popup.x, y: props.popup.y });

  const select = (i: number) => {
    const n = visible().length;
    if (!n) return;
    const next = (i + n) % n;
    setActive(next);
    visible()[next].preview?.();
    list.querySelector<HTMLElement>(`[data-index="${next}"]`)?.scrollIntoView({ block: "nearest" });
  };
  const choose = (item: PopupItem | undefined) => {
    if (!item) return;
    closePopup(false);
    item.run();
  };
  const away = (e: MouseEvent) => {
    if (!el.contains(e.target as Node)) closePopup();
  };
  onMount(() => {
    const r = el.getBoundingClientRect();
    const x = Math.max(8, Math.min(props.popup.x, window.innerWidth - r.width - 8));
    const y = props.popup.y + r.height > window.innerHeight - 8 ? Math.max(8, props.popup.y - r.height - 22) : props.popup.y;
    setPos({ x, y });
    window.addEventListener("mousedown", away, true);
    el.focus();
    select(0);
  });
  onCleanup(() => window.removeEventListener("mousedown", away, true));

  const onKey = (e: KeyboardEvent) => {
    const p = props.popup;
    if (e.key === "ArrowDown") select(active() + 1);
    else if (e.key === "ArrowUp") select(active() - 1);
    else if (e.key === "Enter" || (e.key === "Tab" && p.filterable)) choose(visible()[active()]);
    else if (e.key === "Escape") closePopup();
    else if (e.key === "Backspace" && p.filterable) {
      setState("popup", "filter", p.filter.slice(0, -1));
      select(0);
    } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      if (p.filterable) {
        setState("popup", "filter", p.filter + e.key);
        select(0);
      } else {
        const item = visible().find((i) => i.mnemonic?.toLowerCase() === e.key.toLowerCase());
        if (!item) return;
        choose(item);
      }
    } else return;
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <div
      ref={el}
      class="lt-popup"
      role="listbox"
      aria-label={props.popup.title}
      tabindex={-1}
      style={{ left: `${pos().x}px`, top: `${pos().y}px` }}
      onKeyDown={onKey}
    >
      <div class="lt-popup-title">
        <span>{props.popup.title}</span>
        <Show when={props.popup.filterable && props.popup.filter}>
          <span class="lt-popup-filter">{props.popup.filter}</span>
        </Show>
      </div>
      <div class="lt-popup-list" ref={list}>
        <For each={visible()} fallback={<div class="lt-popup-empty">No matching templates</div>}>
          {(item, i) => (
            <>
              <Show when={item.section && !props.popup.filter}>
                <div class="lt-popup-section">{item.section}</div>
              </Show>
              <div
                class="lt-popup-item"
                role="option"
                data-index={i()}
                data-item={item.id}
                aria-selected={active() === i()}
                classList={{ active: active() === i() }}
                onMouseEnter={() => select(i())}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => choose(item)}
              >
                <Show when={item.mnemonic}>
                  <span class="lt-popup-mnemonic">{item.mnemonic}.</span>
                </Show>
                <span class="lt-popup-label">{item.label}</span>
                <Show when={item.detail}>
                  <span class="lt-popup-detail">{item.detail}</span>
                </Show>
              </div>
            </>
          )}
        </For>
      </div>
    </div>
  );
}

export function LiveTemplatePopupHost() {
  return (
    <Show when={state.popup} keyed>
      {(p) => <TemplatePopup popup={p} />}
    </Show>
  );
}
