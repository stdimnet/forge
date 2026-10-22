/**
 * The language list of "Inject language or reference" (Alt+Enter in a string): Up / Down
 * select, Enter injects, typing filters (speed search ranked in Rust), Escape closes. The
 * choice is written as a `// language=…` comment (`injections_inject`).
 */
import { createMemo, createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { Braces, Code, CodeXml, Database, FileCode, Palette, Regex } from "lucide-solid";
import type { EditorView } from "@codemirror/view";
import { createFuzzyFilter } from "@/features/search/fuzzyFilter";
import { injectionsApi, type InjectableLanguage } from "@/ipc/injections";
import { showBalloon } from "@/keymap";

interface ChooserState {
  view: EditorView;
  path: string;
  pos: number;
  languages: InjectableLanguage[];
  x: number;
  y: number;
}

const [chooser, setChooser] = createSignal<ChooserState | null>(null);

const ICONS: Record<string, (p: { size: number }) => JSX.Element> = {
  sql: Database,
  html: Code,
  css: Palette,
  json: Braces,
  regexp: Regex,
  javascript: FileCode,
  xml: CodeXml,
};

/** Open the language list at the caret of `view`. */
export async function chooseInjectedLanguage(view: EditorView, path: string): Promise<void> {
  const pos = view.state.selection.main.head;
  const languages = await injectionsApi.languages().catch(() => [] as InjectableLanguage[]);
  if (!languages.length) return;
  const coords = view.coordsAtPos(pos);
  setChooser({ view, path, pos, languages, x: Math.max(8, (coords?.left ?? 200) - 24), y: (coords?.bottom ?? 200) + 2 });
}

function close(focus = true): void {
  const c = chooser();
  setChooser(null);
  if (focus) c?.view.focus();
}

async function inject(c: ChooserState, language: InjectableLanguage): Promise<void> {
  close();
  const text = c.view.state.doc.toString();
  const edit = await injectionsApi.inject({ text, path: c.path, offset: c.pos, language: language.id }).catch(() => null);
  if (!edit) {
    showBalloon("Inject language or reference", "Place the caret inside a string literal.", 4000);
    return;
  }
  if (c.view.state.doc.toString() !== text) return;
  c.view.dispatch({ changes: { from: edit.from, to: edit.to, insert: edit.text }, userEvent: "input.inject", scrollIntoView: true });
}

function Chooser(props: { state: ChooserState }) {
  let el!: HTMLDivElement;
  const [query, setQuery] = createSignal("");
  const [selected, setSelected] = createSignal(0);
  const hits = createFuzzyFilter(() => ({ query: query(), candidates: props.state.languages.map((l) => l.label) }));
  const rows = createMemo(() => hits().map((h) => props.state.languages[h.index]));

  const away = (e: MouseEvent) => {
    if (!el.contains(e.target as Node)) close(false);
  };
  onMount(() => {
    window.addEventListener("mousedown", away, true);
    const r = el.getBoundingClientRect();
    if (r.bottom > window.innerHeight - 8) el.style.top = `${Math.max(8, props.state.y - r.height - 24)}px`;
    el.focus();
  });
  onCleanup(() => window.removeEventListener("mousedown", away, true));

  const choose = (l: InjectableLanguage | undefined) => l && void inject(props.state, l);
  // The popup owns these keys while it has focus, like the intention popup.
  const onKey: JSX.EventHandler<HTMLDivElement, KeyboardEvent> = (e) => {
    const n = rows().length;
    if (e.key === "ArrowDown") setSelected((selected() + 1) % Math.max(1, n));
    else if (e.key === "ArrowUp") setSelected((selected() - 1 + Math.max(1, n)) % Math.max(1, n));
    else if (e.key === "Enter") choose(rows()[selected()]);
    else if (e.key === "Escape") {
      if (query()) setQuery("");
      else close();
    } else if (e.key === "Backspace" && query()) {
      setQuery(query().slice(0, -1));
      setSelected(0);
    } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      setQuery(query() + e.key);
      setSelected(0);
    } else return;
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <div
      class="intention-popup injection-chooser"
      ref={el}
      tabIndex={-1}
      role="dialog"
      aria-label="Inject language or reference"
      style={{ left: `${props.state.x}px`, top: `${props.state.y}px` }}
      onKeyDown={onKey}
    >
      <Show when={query()}>
        <div class="intention-search">
          Search for: <span class="intention-query">{query()}</span>
        </div>
      </Show>
      <div class="intention-list" role="listbox">
        <For each={rows()} fallback={<div class="intention-empty">Nothing matches</div>}>
          {(lang, i) => {
            const Icon = ICONS[lang.id] ?? FileCode;
            return (
              <button
                class="intention-row"
                classList={{ selected: i() === selected() }}
                role="option"
                aria-selected={i() === selected()}
                onMouseEnter={() => setSelected(i())}
                onClick={() => choose(lang)}
              >
                <span class="intention-icon injection-language-icon">
                  <Icon size={14} />
                </span>
                <span class="intention-title">{lang.label}</span>
              </button>
            );
          }}
        </For>
      </div>
    </div>
  );
}

/** Host of the popup, rendered once into the document body. */
export function LanguageChooserHost() {
  return <Show when={chooser()}>{(s) => <Chooser state={s()} />}</Show>;
}
