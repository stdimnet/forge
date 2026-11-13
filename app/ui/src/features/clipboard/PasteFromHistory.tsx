/**
 * Paste from History (Ctrl+Shift+V): PHPStorm's "Choose Content to Paste" with the texts
 * copied or cut in Forge, newest first. Typing filters, the preview shows the whole entry,
 * Enter pastes, Delete removes the entry, Escape closes. The list comes from the Rust
 * clipboard history.
 */
import { For, Show, createEffect, createSignal, on, onCleanup, onMount } from "solid-js";
import { ClipboardList, Search } from "lucide-solid";
import { clipboardApi, type ClipEntry, type ClipItem } from "@/ipc/clipboard";
import { suspendKeymap } from "@/keymap";
import { getSetting } from "@/state/settings";
import { MAX_CONTENTS_KEY } from "./clipboard";
import { t } from "@/i18n";

export interface PasteRequest {
  /** Called with the chosen entry, or null when the popup was closed. */
  done: (entry: ClipEntry | null) => void;
}

const [request, setRequest] = createSignal<PasteRequest | null>(null);

/** Open the popup; resolves to the chosen entry (already moved to the top of the history). */
export function choosePasteContent(): Promise<ClipEntry | null> {
  request()?.done(null);
  return new Promise((resolve) => setRequest({ done: resolve }));
}

/** Size hint on the right of a row. */
const lineCount = (item: ClipItem) => (item.lines > 1 ? `${item.lines} lines` : `${item.chars} chars`);

function Popup(props: { req: PasteRequest }) {
  let root!: HTMLDivElement;
  let list!: HTMLDivElement;
  const release = suspendKeymap();
  onCleanup(release);
  const [query, setQuery] = createSignal("");
  const [items, setItems] = createSignal<ClipItem[]>([]);
  const [selected, setSelected] = createSignal(0);
  const [loaded, setLoaded] = createSignal(false);

  async function reload() {
    const max = getSetting<number>(MAX_CONTENTS_KEY, 20);
    const found = await clipboardApi.list(query(), max).catch(() => [] as ClipItem[]);
    setItems(found);
    setSelected((i) => Math.max(0, Math.min(i, found.length - 1)));
    setLoaded(true);
  }

  onMount(() => {
    root.focus();
    void reload();
  });
  createEffect(
    on(
      query,
      () => {
        setSelected(0);
        void reload();
      },
      { defer: true },
    ),
  );
  createEffect(() => list?.querySelector(`[data-index="${selected()}"]`)?.scrollIntoView({ block: "nearest" }));

  const current = () => items()[selected()];

  const finish = (entry: ClipEntry | null) => {
    // Read the request before closing: the popup's props go stale once it is hidden.
    const req = props.req;
    setRequest(null);
    req.done(entry);
  };

  async function choose(item = current()) {
    if (!item) return;
    const entry = (await clipboardApi.promote(item.index).catch(() => null)) ?? { text: item.text, carets: item.carets };
    finish(entry);
  }

  async function removeSelected() {
    const item = current();
    if (!item) return;
    await clipboardApi.remove(item.index).catch(() => false);
    await reload();
  }

  function onKeyDown(e: KeyboardEvent) {
    const move = (d: number) => setSelected((i) => Math.max(0, Math.min(items().length - 1, i + d)));
    switch (e.key) {
      case "ArrowDown":
        move(1);
        break;
      case "ArrowUp":
        move(-1);
        break;
      case "PageDown":
        move(10);
        break;
      case "PageUp":
        move(-10);
        break;
      case "Home":
        setSelected(0);
        break;
      case "End":
        setSelected(Math.max(0, items().length - 1));
        break;
      case "Enter":
        void choose();
        break;
      case "Escape":
        if (query()) setQuery("");
        else finish(null);
        break;
      case "Delete":
        void removeSelected();
        break;
      case "Backspace":
        setQuery((q) => q.slice(0, -1));
        break;
      default:
        if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) setQuery((q) => q + e.key);
        else return;
    }
    e.preventDefault();
    e.stopPropagation();
  }

  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && finish(null)}>
      <div class="popup paste-history-popup" tabIndex={-1} ref={root} role="dialog" aria-label={t("tools.clipboard.chooseContentToPaste")} onKeyDown={onKeyDown}>
        <div class="paste-history-header">
          <ClipboardList size={14} class="muted" />
          <span class="paste-history-title">{t("tools.clipboard.chooseContentToPaste")}</span>
          <Show when={query()}>
            <span class="speed-search">
              <Search size={12} />
              {query()}
            </span>
          </Show>
        </div>
        <div class="paste-history-list" ref={list} role="listbox">
          <Show
            when={items().length}
            fallback={<div class="se-empty">{loaded() ? (query() ? t("common.nothingFound") : t("tools.clipboard.nothingWasCopiedInThis")) : ""}</div>}
          >
            <For each={items()}>
              {(item, i) => (
                <div
                  class="paste-history-row"
                  classList={{ selected: selected() === i() }}
                  role="option"
                  aria-selected={selected() === i()}
                  data-index={i()}
                  onMouseMove={() => setSelected(i())}
                  onClick={() => void choose(item)}
                >
                  <span class="paste-history-number">{i() + 1}</span>
                  <span class="paste-history-text">{item.preview || "␣"}</span>
                  <Show when={(item.carets?.length ?? 0) > 1}>
                    <span class="paste-history-meta">{item.carets!.length} carets</span>
                  </Show>
                  <span class="paste-history-meta">{lineCount(item)}</span>
                </div>
              )}
            </For>
          </Show>
        </div>
        <pre class="paste-history-preview" aria-label={t("tools.clipboard.preview")}>{current()?.text ?? ""}</pre>
        <div class="se-footer">{t("tools.clipboard.enterToPasteDeleteTo")}</div>
      </div>
    </div>
  );
}

export function PasteFromHistoryHost() {
  return <Show when={request()}>{(r) => <Popup req={r()} />}</Show>;
}
