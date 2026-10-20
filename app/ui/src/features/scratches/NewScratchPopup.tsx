/**
 * "New Scratch File" popup (Ctrl+Alt+Shift+Insert): the languages of all file types with
 * speed search. Ordering and matching happen in Rust (`scratches_languages`); the current
 * file's language and recently used ones come first, as in PHPStorm.
 */
import { createEffect, createResource, createSignal, For, on, onMount, Show } from "solid-js";
import { Search } from "lucide-solid";
import { scratchesApi, type ScratchLanguage } from "@/ipc/scratches";
import { languageIdFor } from "@/editor/languages";
import { activeTab } from "@/state/editor";
import { FileIcon } from "@/shell/FileIcon";
import { createScratch, recentLanguages } from "./store";

const [open, setOpen] = createSignal(false);

export const openNewScratchPopup = () => setOpen(true);

export function NewScratchPopupHost() {
  return (
    <Show when={open()}>
      <NewScratchPopup />
    </Show>
  );
}

/** Name with the matched characters highlighted. */
function Highlighted(props: { text: string; positions: number[] }) {
  const parts = () => {
    const set = new Set(props.positions);
    const out: { text: string; hl: boolean }[] = [];
    [...props.text].forEach((ch, i) => {
      const hl = set.has(i);
      const last = out[out.length - 1];
      if (last && last.hl === hl) last.text += ch;
      else out.push({ text: ch, hl });
    });
    return out;
  };
  return <For each={parts()}>{(p) => (p.hl ? <mark class="hl">{p.text}</mark> : p.text)}</For>;
}

function NewScratchPopup() {
  let input!: HTMLInputElement;
  let list!: HTMLDivElement;
  const [query, setQuery] = createSignal("");
  const [index, setIndex] = createSignal(0);
  const tab = activeTab();
  const current = tab ? languageIdFor(tab.path, tab.content) : undefined;
  const [items] = createResource(
    () => ({ query: query(), current, recent: recentLanguages() }),
    (request) => scratchesApi.languages(request),
    { initialValue: [] },
  );
  createEffect(on(query, () => setIndex(0)));
  createEffect(
    on(index, (i) => list?.querySelector(`[data-index="${i}"]`)?.scrollIntoView({ block: "nearest" })),
  );
  onMount(() => input.focus());

  const close = () => setOpen(false);
  const choose = (lang: ScratchLanguage | undefined) => {
    if (!lang) return;
    close();
    void createScratch(lang.id);
  };
  const onKeyDown = (e: KeyboardEvent) => {
    const count = items().length;
    if (e.key === "Escape") close();
    else if (e.key === "ArrowDown") setIndex((i) => (count ? (i + 1) % count : 0));
    else if (e.key === "ArrowUp") setIndex((i) => (count ? (i - 1 + count) % count : 0));
    else if (e.key === "PageDown") setIndex((i) => Math.min(count - 1, i + 10));
    else if (e.key === "PageUp") setIndex((i) => Math.max(0, i - 10));
    else if (e.key === "Enter") choose(items()[index()]);
    else return;
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div class="popup scratch-popup" role="dialog" aria-label="New Scratch File" onKeyDown={onKeyDown}>
        <div class="scratch-popup-title">New Scratch File</div>
        <label class="scratch-popup-search">
          <Search size={14} class="muted" />
          <input
            ref={input}
            value={query()}
            placeholder="Type to search languages"
            spellcheck={false}
            onInput={(e) => setQuery(e.currentTarget.value)}
          />
        </label>
        <div class="scratch-popup-list" ref={list} role="listbox">
          <For each={items()} fallback={<div class="scratch-popup-empty muted">No matches</div>}>
            {(lang, i) => (
              <div
                class="scratch-popup-item"
                classList={{ selected: index() === i() }}
                role="option"
                aria-selected={index() === i()}
                data-index={i()}
                data-language={lang.id}
                onMouseEnter={() => setIndex(i())}
                onClick={() => choose(lang)}
              >
                <FileIcon name={`scratch.${lang.extension}`} size={14} />
                <span class="scratch-popup-name">
                  <Highlighted text={lang.name} positions={lang.positions} />
                </span>
                <span class="scratch-popup-ext muted">.{lang.extension}</span>
              </div>
            )}
          </For>
        </div>
      </div>
    </div>
  );
}
