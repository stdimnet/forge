/**
 * Expression field of the Evaluate dialog and the debugger console: completion of
 * variables (`$`) and members (`->`) from the suspended frame (computed by the backend),
 * and the history of evaluated expressions (Up / Down, or the history button).
 *
 * The keys below are handled on the field itself, like any text field handles its own
 * editing keys; they are not IDE shortcuts.
 */
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { Braces, History, Variable } from "lucide-solid";
import type { Completion } from "./api";
import { completeExpression, expressionHistory, isDapSession } from "./store";
import { t } from "@/i18n";

export interface ExpressionInputProps {
  value: string;
  onInput: (value: string) => void;
  onSubmit: (value: string) => void;
  placeholder?: string;
  id?: string;
  class?: string;
  disabled?: boolean;
  autofocus?: boolean;
  /** Called with the field once mounted (e.g. to select its text). */
  ref?: (el: HTMLInputElement) => void;
}

/** The identifier being typed at the end of `text` (what a completion replaces). */
export const typedPrefix = (text: string): string => /[\p{L}\p{N}_]*$/u.exec(text)?.[0] ?? "";

/**
 * Whether typing `text` should ask for completions: after `$` or `->` in PHP; in debug
 * adapter sessions also on identifiers and after `.`.
 */
export const wantsCompletion = (text: string, dap = false): boolean =>
  dap ? /(?:[\p{L}_][\p{L}\p{N}_]*|\.|->)$/u.test(text) : /(\$|->)[\p{L}\p{N}_]*$/u.test(text);

export function ExpressionInput(props: ExpressionInputProps) {
  let input!: HTMLInputElement;
  const [items, setItems] = createSignal<Completion[]>([]);
  const [active, setActive] = createSignal(0);
  const [historyOpen, setHistoryOpen] = createSignal(false);
  /** Position in the history while browsing with Up/Down (-1: the typed text). */
  let historyIndex = -1;
  let draft = "";
  let timer: ReturnType<typeof setTimeout> | undefined;
  let request = 0;

  const close = () => {
    setItems([]);
    setHistoryOpen(false);
  };
  const requestCompletions = (text: string) => {
    clearTimeout(timer);
    if (!wantsCompletion(text, isDapSession())) return setItems([]);
    const n = ++request;
    timer = setTimeout(async () => {
      const list = await completeExpression(text);
      if (n !== request || props.value !== text) return;
      setItems(list.slice(0, 50));
      setActive(0);
    }, 120);
  };
  const accept = (c: Completion) => {
    const text = props.value;
    const next = text.slice(0, text.length - typedPrefix(text).length) + c.insert;
    props.onInput(next);
    setItems([]);
    input.focus();
  };
  const fromHistory = (dir: 1 | -1) => {
    const h = expressionHistory();
    if (!h.length) return;
    if (historyIndex === -1) draft = props.value;
    historyIndex = Math.max(-1, Math.min(h.length - 1, historyIndex + dir));
    props.onInput(historyIndex === -1 ? draft : h[historyIndex]);
  };
  const onKeyDown = (e: KeyboardEvent) => {
    const list = items();
    if (list.length) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        setActive((i) => (i + (e.key === "ArrowDown" ? 1 : list.length - 1)) % list.length);
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        accept(list[active()]);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        setItems([]);
        return;
      }
    }
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      fromHistory(e.key === "ArrowUp" ? 1 : -1);
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      historyIndex = -1;
      close();
      props.onSubmit(props.value);
    }
  };
  onMount(() => {
    props.ref?.(input);
    if (props.autofocus) input.focus();
  });
  onCleanup(() => clearTimeout(timer));

  return (
    <div class={`dbg-expr ${props.class ?? ""}`}>
      <input
        ref={input}
        id={props.id}
        class="dbg-input dbg-mono dbg-expr-input"
        value={props.value}
        placeholder={props.placeholder}
        disabled={props.disabled}
        spellcheck={false}
        autocomplete="off"
        onInput={(e) => {
          historyIndex = -1;
          props.onInput(e.currentTarget.value);
          requestCompletions(e.currentTarget.value);
        }}
        onKeyDown={onKeyDown}
        onBlur={() => setTimeout(() => setItems([]), 150)}
      />
      <button
        class="icon-btn dbg-expr-history"
        title={t("debug.expressionInput.history")}
        disabled={!expressionHistory().length}
        onMouseDown={(e) => e.preventDefault()}
        onClick={() => setHistoryOpen((o) => !o)}
      >
        <History size={13} />
      </button>
      <Show when={items().length > 0}>
        <div class="dbg-completions" role="listbox">
          <For each={items()}>
            {(c, i) => (
              <div
                class="dbg-completion"
                classList={{ selected: i() === active() }}
                role="option"
                aria-selected={i() === active()}
                onMouseDown={(e) => {
                  e.preventDefault();
                  accept(c);
                }}
              >
                {c.label.startsWith("$") ? <Variable size={12} class="dbg-icon dbg-icon-scalar" /> : <Braces size={12} class="dbg-icon dbg-icon-object" />}
                <span class="dbg-mono">{c.label}</span>
                <span class="muted dbg-completion-detail">{c.detail}</span>
              </div>
            )}
          </For>
        </div>
      </Show>
      <Show when={historyOpen()}>
        <div class="dbg-completions dbg-history" role="listbox">
          <For each={expressionHistory()}>
            {(h) => (
              <div
                class="dbg-completion"
                role="option"
                onMouseDown={(e) => {
                  e.preventDefault();
                  props.onInput(h);
                  setHistoryOpen(false);
                  input.focus();
                }}
              >
                <span class="dbg-mono">{h}</span>
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}
