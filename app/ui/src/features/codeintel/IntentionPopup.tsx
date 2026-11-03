/**
 * PHPStorm's intention popup (Alt+Enter): quick fixes with a red bulb, intentions with a
 * yellow one, refactorings and source actions, links like "Generate..." last. Up / Down
 * select, Enter applies, Right opens an entry's sub-menu ("Fix all ... in file"), Left
 * closes it, typing filters (speed search ranked in Rust), Escape closes.
 */
import { createMemo, createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { ChevronRight, FileCode, Lightbulb, Sparkles, Wrench } from "lucide-solid";
import type { ContextAction } from "@/ipc/codeintel";
import { createFuzzyFilter } from "@/features/search/fuzzyFilter";
import { actionIcon, closeIntentionPopup, runContextAction, type IntentionPopupState } from "./intentions";

function ActionIcon(props: { action: ContextAction }) {
  const kind = () => actionIcon(props.action);
  return (
    <span class={`intention-icon ${kind()}`}>
      <Show when={kind() === "fix" || kind() === "intention"}>
        <Lightbulb size={14} />
      </Show>
      <Show when={kind() === "refactor"}>
        <Wrench size={14} />
      </Show>
      <Show when={kind() === "source"}>
        <FileCode size={14} />
      </Show>
      <Show when={kind() === "link"}>
        <Sparkles size={14} />
      </Show>
    </span>
  );
}

/** A title with the speed-search matches highlighted. */
function Title(props: { text: string; positions: number[] }) {
  const parts = createMemo(() => {
    const set = new Set(props.positions);
    const out: { text: string; hit: boolean }[] = [];
    [...props.text].forEach((ch, i) => {
      const hit = set.has(i);
      const last = out[out.length - 1];
      if (last && last.hit === hit) last.text += ch;
      else out.push({ text: ch, hit });
    });
    return out;
  });
  return (
    <span class="intention-title">
      <For each={parts()}>{(p) => (p.hit ? <mark>{p.text}</mark> : p.text)}</For>
    </span>
  );
}

export function IntentionPopup(props: { popup: IntentionPopupState }) {
  let el!: HTMLDivElement;
  const [query, setQuery] = createSignal("");
  const [selected, setSelected] = createSignal(0);
  /** The entry whose sub-menu is open, and the selection inside it. */
  const [sub, setSub] = createSignal<{ index: number; selected: number } | null>(null);

  const hits = createFuzzyFilter(() => ({ query: query(), candidates: props.popup.actions.map((a) => a.title) }));
  const rows = createMemo(() => hits().map((h) => ({ action: props.popup.actions[h.index], positions: h.positions })));
  const current = () => rows()[selected()]?.action;

  const away = (e: MouseEvent) => {
    if (!el.contains(e.target as Node)) closeIntentionPopup(false);
  };
  onMount(() => {
    window.addEventListener("mousedown", away, true);
    const r = el.getBoundingClientRect();
    if (r.bottom > window.innerHeight - 8) el.style.top = `${Math.max(8, props.popup.y - r.height - 24)}px`;
    if (r.right > window.innerWidth - 8) el.style.left = `${Math.max(8, window.innerWidth - r.width - 8)}px`;
    el.focus();
  });
  onCleanup(() => window.removeEventListener("mousedown", away, true));

  const choose = (a: ContextAction | undefined) => {
    if (a) void runContextAction(props.popup.path, props.popup.view, a);
  };
  const move = (delta: number) => {
    const s = sub();
    if (s) {
      const n = current()?.children?.length ?? 0;
      if (n) setSub({ ...s, selected: (s.selected + delta + n) % n });
      return;
    }
    const n = rows().length;
    if (n) setSelected((selected() + delta + n) % n);
  };
  const openSub = () => {
    if (current()?.children?.length) setSub({ index: selected(), selected: 0 });
  };

  // The popup owns these keys while it has focus (like the Show Usages popup).
  const onKey: JSX.EventHandler<HTMLDivElement, KeyboardEvent> = (e) => {
    const s = sub();
    if (e.key === "ArrowDown") move(1);
    else if (e.key === "ArrowUp") move(-1);
    else if (e.key === "ArrowRight") openSub();
    else if (e.key === "ArrowLeft" && s) setSub(null);
    else if (e.key === "Enter") choose(s ? current()?.children?.[s.selected] : current());
    else if (e.key === "Escape") {
      if (s) setSub(null);
      else if (query()) setQuery("");
      else closeIntentionPopup();
    } else if (e.key === "Backspace" && query()) {
      setQuery(query().slice(0, -1));
      setSelected(0);
    } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      setQuery(query() + e.key);
      setSelected(0);
      setSub(null);
    } else return;
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <div
      class="intention-popup"
      ref={el}
      tabIndex={-1}
      role="dialog"
      aria-label="Context actions"
      style={{ left: `${props.popup.x}px`, top: `${props.popup.y}px` }}
      onKeyDown={onKey}
    >
      <Show when={query()}>
        <div class="intention-search">
          Search for: <span class="intention-query">{query()}</span>
        </div>
      </Show>
      <div class="intention-list" role="listbox">
        <For each={rows()} fallback={<div class="intention-empty">Nothing matches</div>}>
          {(row, i) => (
            <div class="intention-row-wrap">
              <button
                class="intention-row"
                classList={{ selected: i() === selected(), disabled: !!row.action.disabled }}
                role="option"
                aria-selected={i() === selected()}
                title={row.action.disabled ?? undefined}
                onMouseEnter={() => {
                  setSelected(i());
                  if (sub() && sub()!.index !== i()) setSub(null);
                }}
                onClick={() => choose(row.action)}
              >
                <ActionIcon action={row.action} />
                <Title text={row.action.title} positions={row.positions} />
                <Show when={row.action.children?.length}>
                  <span
                    class="intention-more"
                    onClick={(e) => {
                      e.stopPropagation();
                      setSelected(i());
                      openSub();
                    }}
                  >
                    <ChevronRight size={14} />
                  </span>
                </Show>
              </button>
              <Show when={sub()?.index === i() && row.action.children}>
                {(children) => (
                  <div class="intention-submenu" role="listbox">
                    <For each={children()}>
                      {(child, j) => (
                        <button
                          class="intention-row"
                          classList={{ selected: sub()?.selected === j() }}
                          role="option"
                          onMouseEnter={() => setSub({ index: i(), selected: j() })}
                          onClick={() => choose(child)}
                        >
                          <ActionIcon action={child} />
                          <span class="intention-title">{child.title}</span>
                        </button>
                      )}
                    </For>
                  </div>
                )}
              </Show>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}
