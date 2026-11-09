import { For, Show, createEffect, createMemo, createSignal, on, onCleanup, onMount, type JSX } from "solid-js";
import { Dynamic } from "solid-js/web";
import { Filter, Pin, Search, WholeWord } from "lucide-solid";
import { exactMatching, setExactOverride } from "@/ipc/search";
import { shortcutLabel } from "@/keymap";
import {
  SEARCH_TABS,
  runSearchStreaming,
  type SearchGroup,
  type SearchItem,
  type SearchItemKind,
} from "./providers";
import {
  closeSearchEverywhere,
  seIncludeNonProject,
  seQuery,
  seTab,
  setSeIncludeNonProject,
  setSeQuery,
  setSeTab,
  type SeTab,
} from "./state";
import { t, tOr } from "@/i18n";
import { ThemedSymbolIcon } from "@/features/iconThemes/UiIcon";

/** Render `text` with matched characters emphasized. */
export function Highlight(props: { text: string; positions?: number[] }) {
  const parts = createMemo(() => {
    const set = new Set(props.positions ?? []);
    const out: { t: string; hit: boolean }[] = [];
    [...props.text].forEach((ch, i) => {
      const hit = set.has(i);
      const last = out[out.length - 1];
      if (last && last.hit === hit) last.t += ch;
      else out.push({ t: ch, hit });
    });
    return out;
  });
  return <For each={parts()}>{(p) => (p.hit ? <mark class="hl">{p.t}</mark> : p.t)}</For>;
}

const KIND_BADGE: Partial<Record<SearchItemKind, { letter: string; cls: string }>> = {
  class: { letter: "C", cls: "k-class" },
  interface: { letter: "I", cls: "k-interface" },
  trait: { letter: "T", cls: "k-trait" },
  enum: { letter: "E", cls: "k-enum" },
  function: { letter: "f", cls: "k-function" },
  method: { letter: "m", cls: "k-method" },
  property: { letter: "p", cls: "k-property" },
  constant: { letter: "c", cls: "k-constant" },
};

export function ItemIcon(props: { item: SearchItem }): JSX.Element {
  const badge = () => (props.item.kind ? KIND_BADGE[props.item.kind] : undefined);
  return (
    <span class="se-icon">
      <Show
        when={props.item.icon}
        fallback={
          <Show when={badge()}>
            {(b) => <ThemedSymbolIcon kind={props.item.kind!} size={16} fallback={<span class={`kind-badge ${b().cls}`}>{b().letter}</span>} />}
          </Show>
        }
      >
        {(icon) => <Dynamic component={icon()} />}
      </Show>
    </span>
  );
}

type Row =
  | { type: "header"; group: SearchGroup }
  | { type: "item"; item: SearchItem; group: SearchGroup }
  | { type: "more"; group: SearchGroup };

const TAB_ACTION: Record<SeTab, string> = {
  all: "SearchEverywhere",
  classes: "GotoClass",
  files: "GotoFile",
  symbols: "GotoSymbol",
  actions: "GotoAction",
  text: "",
};

export function SearchEverywhere() {
  let input!: HTMLInputElement;
  let list!: HTMLDivElement;
  const [groups, setGroups] = createSignal<SearchGroup[]>([]);
  const [selected, setSelected] = createSignal(0);
  const [loading, setLoading] = createSignal(false);
  let seq = 0;

  const filterLabel = () => (seTab() === "actions" ? t("search.includeDisabled") : t("search.includeNonProject"));

  async function refresh() {
    const id = ++seq;
    const tab = seTab();
    setLoading(true);
    const perGroup = tab === "all" ? 5 : 60;
    // Ranked in Rust; disabled actions only show when asked for or when nothing else matches.
    try {
      await runSearchStreaming(tab, seQuery(), { includeNonProject: seIncludeNonProject(), perGroup }, (res, done) => {
        if (id !== seq) return;
        setGroups(res);
        if (done) setLoading(false);
      });
    } catch (e) {
      console.warn("search failed", e);
    }
    if (id !== seq) return;
    // A failed search never reports `done`: the spinner must not keep turning.
    setLoading(false);
    setSelected(0);
  }

  let timer: ReturnType<typeof setTimeout> | undefined;
  createEffect(
    on([seQuery, seTab, seIncludeNonProject, exactMatching], () => {
      clearTimeout(timer);
      timer = setTimeout(refresh, 60);
    }),
  );
  onCleanup(() => clearTimeout(timer));

  const rows = createMemo<Row[]>(() => {
    const out: Row[] = [];
    const showHeaders = seTab() === "all" || groups().length > 1;
    for (const g of groups()) {
      if (showHeaders) out.push({ type: "header", group: g });
      for (const item of g.items) out.push({ type: "item", item, group: g });
      if (g.more && seTab() === "all") out.push({ type: "more", group: g });
    }
    return out;
  });
  const selectable = createMemo(() => rows().map((r, i) => (r.type === "header" ? -1 : i)).filter((i) => i >= 0));
  // Streamed results can shrink the list under the selection: keep it on a row.
  createEffect(() => {
    const n = selectable().length;
    if (selected() >= n) setSelected(Math.max(0, n - 1));
  });

  createEffect(() => {
    const idx = selectable()[selected()];
    list?.querySelector(`[data-row="${idx}"]`)?.scrollIntoView({ block: "nearest" });
  });

  async function activate(row: Row | undefined) {
    if (!row) return;
    if (row.type === "more") {
      setSeTab(row.group.provider.tab);
      input.focus();
      return;
    }
    if (row.type === "item") {
      closeSearchEverywhere();
      await row.item.run();
    }
  }

  const switchTab = (dir: 1 | -1) => {
    const ids = SEARCH_TABS.map((t) => t.id);
    setSeTab(ids[(ids.indexOf(seTab()) + dir + ids.length) % ids.length]);
  };

  function onKeyDown(e: KeyboardEvent) {
    const n = selectable().length;
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        if (n) setSelected((s) => (s + 1) % n);
        break;
      case "ArrowUp":
        e.preventDefault();
        if (n) setSelected((s) => (s - 1 + n) % n);
        break;
      case "PageDown":
        e.preventDefault();
        setSelected((s) => Math.min(n - 1, s + 10));
        break;
      case "PageUp":
        e.preventDefault();
        setSelected((s) => Math.max(0, s - 10));
        break;
      case "Enter":
        e.preventDefault();
        void activate(rows()[selectable()[selected()]]);
        break;
      case "Escape":
        e.preventDefault();
        closeSearchEverywhere();
        break;
      case "Tab":
        e.preventDefault();
        switchTab(e.shiftKey ? -1 : 1);
        break;
    }
  }

  onMount(() => {
    input.focus();
    input.select();
  });

  const footer = () => {
    const action = TAB_ACTION[seTab()];
    const key = action ? shortcutLabel(action) : "";
    if (!key) return t("search.footer.tab");
    return seTab() === "actions"
      ? t("search.footer.disabled", { key })
      : t("search.footer.nonProject", { key });
  };

  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closeSearchEverywhere()}>
      <div class="popup se-popup" role="dialog" aria-label={t("search.title")} onKeyDown={onKeyDown}>
        <div class="se-header">
          <div class="se-tabs" role="tablist">
            <For each={SEARCH_TABS}>
              {(tab) => (
                <button
                  class="se-tab"
                  classList={{ active: seTab() === tab.id }}
                  role="tab"
                  aria-selected={seTab() === tab.id}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => {
                    setSeTab(tab.id);
                    input.focus();
                  }}
                >
                  {tOr(`search.tab.${tab.id}`, tab.title)}
                </button>
              )}
            </For>
          </div>
          <label class="se-check" onMouseDown={(e) => e.preventDefault()}>
            <input
              type="checkbox"
              checked={seIncludeNonProject()}
              onChange={(e) => {
                setSeIncludeNonProject(e.currentTarget.checked);
                input.focus();
              }}
            />
            <span>{filterLabel()}</span>
          </label>
          <button class="icon-btn se-tool" title={t("search.filter")} aria-label={t("search.filter")} onMouseDown={(e) => e.preventDefault()}>
            <Filter size={14} />
          </button>
          <button class="icon-btn se-tool" title={t("search.openInFind")} aria-label={t("search.openInFind")} onMouseDown={(e) => e.preventDefault()}>
            <Pin size={14} />
          </button>
        </div>
        <div class="se-search">
          <Search size={16} class="muted" />
          <input
            ref={input}
            class="se-input"
            value={seQuery()}
            spellcheck={false}
            autocomplete="off"
            placeholder={seTab() === "actions" ? t("search.typeActionName") : ""}
            role="combobox"
            aria-label={t("search.title")}
            aria-expanded={rows().length > 0}
            aria-controls="se-results"
            aria-autocomplete="list"
            aria-activedescendant={rows().length && selectable().length ? `se-row-${selectable()[selected()]}` : undefined}
            onInput={(e) => setSeQuery(e.currentTarget.value)}
          />
          <Show when={loading()}>
            <span class="se-spinner" />
          </Show>
          <button
            class="icon-btn se-tool"
            classList={{ active: exactMatching() }}
            title={exactMatching() ? t("search.matchContains") : t("search.matchFuzzy")}
            aria-pressed={exactMatching()}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => {
              setExactOverride(!exactMatching());
              input.focus();
            }}
          >
            <WholeWord size={14} />
          </button>
        </div>
        <div
          class="se-results"
          ref={list}
          id="se-results"
          role={rows().length ? "listbox" : undefined}
          aria-label={t("a11y.searchResults")}
          tabIndex={-1}
        >
          <Show
            when={rows().length}
            fallback={
              <div class="se-empty">
                {seQuery().trim() ? t("search.nothingFound") : seTab() === "actions" ? t("search.typeToSearchActions") : t("search.typeToSearch")}
              </div>
            }
          >
            <For each={rows()}>
              {(row, i) => {
                const isSel = () => selectable()[selected()] === i();
                const selectRow = () => {
                  const idx = selectable().indexOf(i());
                  if (idx >= 0) setSelected(idx);
                };
                if (row.type === "header")
                  return (
                    <div class="se-group" role="presentation">
                      {row.group.provider.title}
                    </div>
                  );
                if (row.type === "more")
                  return (
                    <div
                      class="se-row se-more"
                      classList={{ selected: isSel() }}
                      data-row={i()}
                      id={`se-row-${i()}`}
                      role="option"
                      aria-selected={isSel()}
                      onMouseMove={selectRow}
                      onClick={() => activate(row)}
                    >
                      {t("search.more")}
                    </div>
                  );
                const item = row.item;
                return (
                  <div
                    class="se-row"
                    classList={{ selected: isSel(), dimmed: !!item.dimmed }}
                    data-row={i()}
                    id={`se-row-${i()}`}
                    role="option"
                    aria-selected={isSel()}
                    onMouseMove={selectRow}
                    onClick={() => activate(row)}
                  >
                    <ItemIcon item={item} />
                    <span class="se-label">
                      <Highlight text={item.label} positions={item.matches} />
                    </span>
                    <Show when={item.detail}>
                      <span class="se-detail">{item.detail}</span>
                    </Show>
                    <span class="se-hint" classList={{ shortcut: item.kind === "action" }}>
                      {item.hint}
                    </span>
                  </div>
                );
              }}
            </For>
          </Show>
        </div>
        <div class="se-footer">{footer()}</div>
      </div>
    </div>
  );
}
