/**
 * PHPStorm's "Enter Abbreviation" popup for Surround with Emmet and Update Tag with Emmet:
 * a field below the caret with the abbreviations used before (Up/Down) and, while typing,
 * a preview of the result computed in Rust. Enter applies, Escape closes.
 */
import { createMemo, createSignal, onCleanup, onMount, Show } from "solid-js";
import { createStore } from "solid-js/store";
import type { EditorView } from "@codemirror/view";
import { t } from "@/i18n";

export interface AbbreviationPopupOptions {
  title: string;
  /** Result for the current abbreviation: text to show, or an error. */
  preview?: (abbreviation: string) => { text?: string; error?: string } | null;
  submit: (abbreviation: string) => void | Promise<void>;
}

interface PopupState extends AbbreviationPopupOptions {
  view: EditorView;
  x: number;
  y: number;
}

const [state, setState] = createStore<{ popup: PopupState | null }>({ popup: null });

/** Abbreviations entered in this session, newest first. */
const history: string[] = [];
const MAX_HISTORY = 20;

export const abbreviationPopupOpen = () => !!state.popup;

export function openAbbreviationPopup(view: EditorView, opts: AbbreviationPopupOptions): void {
  const sel = view.state.selection.main;
  const c = view.coordsAtPos(sel.from) ?? view.coordsAtPos(sel.head);
  setState("popup", { ...opts, view, x: c ? c.left : 200, y: c ? c.bottom + 4 : 200 });
}

export function closeAbbreviationPopup(focusEditor = true): void {
  const p = state.popup;
  if (!p) return;
  setState("popup", null);
  if (focusEditor) p.view.focus();
}

const MAX_PREVIEW_LINES = 14;

function Popup(props: { popup: PopupState }) {
  let el!: HTMLDivElement;
  let input!: HTMLInputElement;
  const [value, setValue] = createSignal("");
  const [pos, setPos] = createSignal({ x: props.popup.x, y: props.popup.y });
  let historyIndex = -1;

  const preview = createMemo(() => {
    const abbr = value().trim();
    if (!abbr || !props.popup.preview) return null;
    try {
      return props.popup.preview(abbr);
    } catch (e) {
      return { error: String(e instanceof Error ? e.message : e) };
    }
  });
  const previewText = () => {
    const lines = (preview()?.text ?? "").split("\n");
    return lines.slice(0, MAX_PREVIEW_LINES).join("\n") + (lines.length > MAX_PREVIEW_LINES ? "\n…" : "");
  };

  const away = (e: MouseEvent) => {
    if (!el.contains(e.target as Node)) closeAbbreviationPopup();
  };
  onMount(() => {
    const r = el.getBoundingClientRect();
    const x = Math.max(8, Math.min(props.popup.x, window.innerWidth - r.width - 8));
    setPos({ x, y: props.popup.y });
    window.addEventListener("mousedown", away, true);
    input.focus();
  });
  onCleanup(() => window.removeEventListener("mousedown", away, true));

  const submit = () => {
    const abbr = value().trim();
    if (!abbr) return;
    const i = history.indexOf(abbr);
    if (i >= 0) history.splice(i, 1);
    history.unshift(abbr);
    history.length = Math.min(history.length, MAX_HISTORY);
    const p = props.popup;
    closeAbbreviationPopup(false);
    void p.submit(abbr);
  };

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Enter") submit();
    else if (e.key === "Escape") closeAbbreviationPopup();
    else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      if (!history.length) return;
      historyIndex = Math.max(-1, Math.min(history.length - 1, historyIndex + (e.key === "ArrowUp" ? 1 : -1)));
      setValue(historyIndex < 0 ? "" : history[historyIndex]);
    } else return;
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <div ref={el} class="emmet-popup" role="dialog" aria-label={props.popup.title} style={{ left: `${pos().x}px`, top: `${pos().y}px` }}>
      <div class="emmet-popup-title">{props.popup.title}</div>
      <input
        ref={input}
        class="emmet-popup-input"
        spellcheck={false}
        autocomplete="off"
        placeholder={t("tools.emmet.enterAbbreviation")}
        value={value()}
        onInput={(e) => {
          historyIndex = -1;
          setValue(e.currentTarget.value);
        }}
        onKeyDown={onKey}
      />
      <Show when={preview()}>
        {(p) => (
          <Show when={p().text !== undefined} fallback={<div class="emmet-popup-error">{p().error}</div>}>
            <pre class="emmet-popup-preview">{previewText()}</pre>
          </Show>
        )}
      </Show>
    </div>
  );
}

export function AbbreviationPopupHost() {
  return (
    <Show when={state.popup} keyed>
      {(p) => <Popup popup={p} />}
    </Show>
  );
}
