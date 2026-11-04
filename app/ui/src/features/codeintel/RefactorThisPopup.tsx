/**
 * The Refactor This popup (`refactorThis.ts`): numbered rows with their shortcuts; a digit
 * picks a row, typing filters (speed search), arrows and Enter choose, Escape closes.
 */
import { createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { indexOfMnemonic, matches, mnemonic, type RefactorThisRequest } from "./refactorThis";
import { t } from "@/i18n";

export function RefactorThisPopup(props: { req: RefactorThisRequest }) {
  let el!: HTMLDivElement;
  const [query, setQuery] = createSignal("");
  const [selected, setSelected] = createSignal(0);
  const rows = createMemo(() => props.req.items.map((item, i) => ({ item, i })).filter(({ item }) => matches(item.title, query())));
  const away = (e: MouseEvent) => {
    if (!el.contains(e.target as Node)) props.req.done(null);
  };
  onMount(() => {
    window.addEventListener("mousedown", away, true);
    const r = el.getBoundingClientRect();
    if (r.bottom > window.innerHeight - 8) el.style.top = `${Math.max(8, props.req.y - r.height - 28)}px`;
    if (r.right > window.innerWidth - 8) el.style.left = `${Math.max(8, window.innerWidth - r.width - 8)}px`;
    el.focus();
  });
  onCleanup(() => window.removeEventListener("mousedown", away, true));
  const pick = (k: number) => {
    const row = rows()[k];
    if (row) props.req.done(row.item);
  };
  const onKeyDown = (e: KeyboardEvent) => {
    const n = rows().length;
    if (e.key === "ArrowDown") setSelected(n ? (selected() + 1) % n : 0);
    else if (e.key === "ArrowUp") setSelected(n ? (selected() - 1 + n) % n : 0);
    else if (e.key === "Enter") pick(selected());
    else if (e.key === "Escape") {
      if (query()) setQuery("");
      else props.req.done(null);
    } else if (e.key === "Backspace") setQuery(query().slice(0, -1));
    else if (!query() && indexOfMnemonic(e.key, props.req.items.length) != null) {
      const at = rows().findIndex((r) => r.i === indexOfMnemonic(e.key, props.req.items.length));
      if (at >= 0) pick(at);
    } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      setQuery(query() + e.key);
      setSelected(0);
    } else return;
    e.preventDefault();
    e.stopPropagation();
  };
  return (
    <div
      class="intention-popup refactor-chooser refactor-this"
      ref={el}
      tabIndex={-1}
      role="dialog"
      aria-label={t("codeintel.refactorThisPopup.refactorThis")}
      style={{ left: `${props.req.x}px`, top: `${props.req.y}px` }}
      onKeyDown={onKeyDown}
    >
      <div class="refactor-chooser-title">{t("codeintel.refactorThisPopup.refactorThis")}</div>
      <div class="intention-list" role="listbox">
        <For each={rows()} fallback={<div class="refactor-this-empty">{t("codeintel.refactorThisPopup.nothingMatches")}</div>}>
          {(row, k) => (
            <>
              <Show when={row.item.separator && !query()}>
                <div class="refactor-this-separator" role="separator" />
              </Show>
              <button
                class="intention-row"
                classList={{ selected: k() === selected() }}
                role="option"
                aria-selected={k() === selected()}
                onMouseEnter={() => setSelected(k())}
                onClick={() => props.req.done(row.item)}
              >
                <span class="refactor-this-number">{mnemonic(row.i)}</span>
                <span class="intention-title">{row.item.title}</span>
                <Show when={row.item.shortcut}>
                  <span class="refactor-chooser-detail">{row.item.shortcut}</span>
                </Show>
              </button>
            </>
          )}
        </For>
      </div>
      <Show when={query()}>
        <div class="refactor-this-search">
          Search for: <mark>{query()}</mark>
        </div>
      </Show>
    </div>
  );
}
