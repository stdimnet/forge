/**
 * PHPStorm's per-file Find/Replace bar at the top of the editor (Ctrl+F collapsed, Ctrl+R
 * expanded): search field with history, multi-line toggle and Cc / W / .* toggles, the `n/m`
 * counter, previous/next, Select All Occurrences, In Selection, and the replace row with
 * Replace, Replace All, Exclude and Preserve Case. A CodeMirror panel rendered with Solid.
 */
import { createSignal, For, onCleanup, Show, type JSX } from "solid-js";
import { render } from "solid-js/web";
import { Decoration, EditorView, type Panel, type ViewUpdate } from "@codemirror/view";
import { findNext, findPrevious, getSearchQuery, search, setSearchQuery } from "@codemirror/search";
import { ArrowDown, ArrowUp, ChevronDown, ChevronRight, ReplaceAll, Search, TextCursorInput, TextSelect, WrapText, X } from "lucide-solid";
import { formatForKeymap, shortcutLabel, shortcutsOf, strokeOfEvent } from "@/keymap";
import { t } from "@/i18n";
import {
  controllers,
  closeFindBar,
  excludeCurrent,
  preserveCaseOn,
  recentFind,
  recentReplace,
  rememberHistory,
  rememberOptions,
  replaceAllMatches,
  replaceCurrent,
  selectAllMatches,
  setPreserveCase,
  updateSearch,
} from "./commands";
import { clearExcluded, countMatches, excludeMatch, findScopeField, optionsOf, regexError, setFindScope, type Counter, type FindOptions } from "./logic";
import "./findBar.css";

/** Keys the bar's fields handle themselves; the keymap leaves them alone there. */
const OWN_KEYS = "Alt+Enter Ctrl+Shift+Enter Alt+Down";

let ids = 0;

/** The text of the counter, and whether the field shows the error state. */
export function counterText(c: Counter, o: Pick<FindOptions, "search" | "regexp">): { text: string; error: boolean } {
  if (!o.search) return { text: "", error: false };
  if (regexError(o)) return { text: t("find.bar.badPattern"), error: true };
  if (!c.total) return { text: t("find.bar.noResults"), error: true };
  const plus = c.capped ? "+" : "";
  if (!c.current) return { text: t("find.bar.results", { count: c.total, plus }), error: false };
  return { text: t("find.bar.counter", { current: c.current, total: c.total, plus }), error: false };
}

function HistoryList(props: { id: string; items: string[]; onPick: (s: string) => void; onClose: () => void }) {
  const [active, setActive] = createSignal(0);
  let list!: HTMLDivElement;
  queueMicrotask(() => list?.focus());
  const onKeyDown = (e: KeyboardEvent) => {
    const n = props.items.length;
    if (e.key === "ArrowDown") setActive((i) => Math.min(n - 1, i + 1));
    else if (e.key === "ArrowUp") setActive((i) => Math.max(0, i - 1));
    else if (e.key === "Enter" && n) props.onPick(props.items[active()]);
    else if (e.key === "Escape") props.onClose();
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
  return (
    <div
      ref={list}
      id={props.id}
      class="fb-history"
      role="listbox"
      tabIndex={0}
      aria-label={t("find.bar.recent")}
      aria-activedescendant={props.items.length ? `${props.id}-${active()}` : undefined}
      onKeyDown={onKeyDown}
      onFocusOut={(e) => !list.contains(e.relatedTarget as Node) && props.onClose()}
    >
      <Show when={props.items.length} fallback={<div class="fb-history-empty">{t("find.bar.noRecent")}</div>}>
        <For each={props.items}>
          {(item, i) => (
            <div
              id={`${props.id}-${i()}`}
              class="fb-history-row"
              classList={{ active: i() === active() }}
              role="option"
              aria-selected={i() === active()}
              onMouseEnter={() => setActive(i())}
              onMouseDown={(e) => {
                e.preventDefault();
                props.onPick(item);
              }}
            >
              {item.replace(/\n/g, "⏎")}
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}

function FindBar(props: { view: EditorView; state: BarState }) {
  const { view, state: s } = props;
  const id = `fb-${++ids}`;
  let field!: HTMLTextAreaElement;
  let replaceField: HTMLTextAreaElement | undefined;
  const [history, setHistory] = createSignal<"find" | "replace" | null>(null);

  const opts = s.options;
  const set = (patch: Partial<FindOptions>, incremental = false) => {
    const next = { ...opts(), ...patch };
    updateSearch(view, next, incremental);
    if ("caseSensitive" in patch || "wholeWord" in patch || "regexp" in patch) rememberOptions(next);
  };
  const status = () => counterText(s.counter(), opts());
  const errorText = () => regexError(opts());

  const bound = (e: KeyboardEvent, action: string) => {
    const stroke = strokeOfEvent(e);
    return !!stroke && shortcutsOf(action).includes(stroke);
  };
  const insertNewline = (el: HTMLTextAreaElement) => {
    el.setRangeText("\n", el.selectionStart, el.selectionEnd, "end");
    el.dispatchEvent(new Event("input", { bubbles: true }));
    s.setMultiline(true);
  };

  // Keys inside the bar's fields (the keymap's Find / F3 / Shift+F3 still apply).
  const onFieldKey = (e: KeyboardEvent, which: "find" | "replace") => {
    const el = e.currentTarget as HTMLTextAreaElement;
    if (e.key === "Enter" && e.altKey && !e.ctrlKey && !e.shiftKey) selectAllMatches(view);
    else if (bound(e, "SelectAllOccurrences")) selectAllMatches(view);
    else if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && e.shiftKey) insertNewline(el);
    else if (e.key === "Enter" && !e.ctrlKey && !e.metaKey && !e.altKey) {
      if (which === "replace") replaceCurrent(view);
      else {
        rememberHistory(view, false);
        if (e.shiftKey) findPrevious(view);
        else findNext(view);
      }
    } else if (e.key === "ArrowDown" && e.altKey) setHistory(which);
    else if (bound(e, "Replace")) {
      s.setReplaceOpen(true);
      queueMicrotask(() => replaceField?.focus());
    } else return;
    e.preventDefault();
    e.stopPropagation();
  };
  const onBarKey = (e: KeyboardEvent) => {
    if (e.key !== "Escape" || history()) return;
    e.preventDefault();
    e.stopPropagation();
    closeFindBar(view);
  };

  const toggle = (key: "caseSensitive" | "wholeWord" | "regexp", label: string, title: string) => (
    <button
      type="button"
      class="find-toggle"
      classList={{ on: opts()[key] }}
      title={title}
      aria-label={title}
      aria-pressed={opts()[key]}
      onMouseDown={(e) => e.preventDefault()}
      onClick={() => set({ [key]: !opts()[key] }, true)}
    >
      {label}
    </button>
  );
  const iconButton = (title: string, icon: JSX.Element, run: () => void, extra: { pressed?: boolean; disabled?: boolean } = {}) => (
    <button
      type="button"
      class="fb-icon"
      classList={{ on: !!extra.pressed }}
      title={title}
      aria-label={title}
      aria-pressed={extra.pressed}
      disabled={extra.disabled}
      onMouseDown={(e) => e.preventDefault()}
      onClick={run}
    >
      {icon}
    </button>
  );
  const withShortcut = (text: string, action: string) => (shortcutLabel(action) ? t("find.bar.withShortcut", { text, shortcut: shortcutLabel(action) }) : text);

  const toggleInSelection = () => {
    if (s.inSelection()) {
      view.dispatch({ effects: setFindScope.of(null) });
      return;
    }
    const ranges = view.state.selection.ranges.filter((r) => !r.empty).map((r) => ({ from: r.from, to: r.to }));
    if (ranges.length) view.dispatch({ effects: setFindScope.of(ranges) });
  };

  s.focus = () => {
    field.focus();
    field.select();
  };
  onCleanup(() => (s.focus = () => undefined));

  return (
    <div class="fb" role="search" aria-label={t("find.bar.title")} onKeyDown={onBarKey}>
      <div class="fb-row">
        <button
          type="button"
          class="fb-icon fb-expand"
          title={s.replaceOpen() ? t("find.bar.collapse") : t("find.bar.expand")}
          aria-label={t("find.bar.toggleReplace")}
          aria-expanded={s.replaceOpen()}
          aria-controls={`${id}-replace`}
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => s.setReplaceOpen(!s.replaceOpen())}
        >
          <Show when={s.replaceOpen()} fallback={<ChevronRight size={14} aria-hidden="true" />}>
            <ChevronDown size={14} aria-hidden="true" />
          </Show>
        </button>
        <div class="fb-field" classList={{ error: status().error, multiline: s.multiline() }}>
          <button
            type="button"
            class="fb-history-button"
            title={t("find.bar.history", { shortcut: formatForKeymap("Alt+Down") })}
            aria-label={t("find.bar.recent")}
            aria-haspopup="listbox"
            aria-expanded={history() === "find"}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => setHistory(history() === "find" ? null : "find")}
          >
            <Search size={14} aria-hidden="true" />
            <ChevronDown size={10} aria-hidden="true" />
          </button>
          <textarea
            ref={field}
            main-field="true"
            class="fb-input"
            rows={s.multiline() ? 3 : 1}
            value={opts().search}
            spellcheck={false}
            autocomplete="off"
            aria-label={t("find.bar.search")}
            aria-invalid={status().error}
            aria-describedby={`${id}-status`}
            data-own-keys={OWN_KEYS}
            title={errorText() ?? ""}
            onInput={(e) => set({ search: e.currentTarget.value }, true)}
            onKeyDown={(e) => onFieldKey(e, "find")}
          />
          {iconButton(t("find.bar.multiline"), <WrapText size={14} aria-hidden="true" />, () => s.setMultiline(!s.multiline()), {
            pressed: s.multiline(),
          })}
          {toggle("caseSensitive", "Cc", t("find.matchCase"))}
          {toggle("wholeWord", "W", t("find.words"))}
          {toggle("regexp", ".*", t("find.regex"))}
          <Show when={history() === "find"}>
            <HistoryList
              id={`${id}-history`}
              items={recentFind()}
              onPick={(text) => {
                setHistory(null);
                set({ search: text }, true);
                field.focus();
              }}
              onClose={() => {
                setHistory(null);
                field.focus();
              }}
            />
          </Show>
        </div>
        <span id={`${id}-status`} class="fb-status" classList={{ error: status().error }} role="status" title={errorText() ?? ""}>
          {status().text}
        </span>
        {iconButton(withShortcut(t("find.bar.previous"), "FindPrevious"), <ArrowUp size={14} aria-hidden="true" />, () => findPrevious(view), {
          disabled: !s.counter().total,
        })}
        {iconButton(withShortcut(t("find.bar.next"), "FindNext"), <ArrowDown size={14} aria-hidden="true" />, () => findNext(view), {
          disabled: !s.counter().total,
        })}
        {iconButton(t("find.bar.withShortcut", { text: t("find.bar.selectAll"), shortcut: formatForKeymap("Alt+Enter") }), <TextCursorInput size={14} aria-hidden="true" />, () => selectAllMatches(view), {
          disabled: !s.counter().total,
        })}
        <span class="fb-sep" aria-hidden="true" />
        {iconButton(t("find.bar.inSelection"), <TextSelect size={14} aria-hidden="true" />, toggleInSelection, {
          pressed: s.inSelection(),
          disabled: !s.inSelection() && !s.hasSelection(),
        })}
        <span class="fb-spacer" />
        {iconButton(t("find.bar.withShortcut", { text: t("find.bar.close"), shortcut: formatForKeymap("Escape") }), <X size={14} aria-hidden="true" />, () => closeFindBar(view))}
      </div>
      <Show when={s.replaceOpen()}>
        <div class="fb-row" id={`${id}-replace`}>
          <span class="fb-indent" aria-hidden="true" />
          <div class="fb-field" classList={{ multiline: s.multiline() }}>
            <button
              type="button"
              class="fb-history-button"
              title={t("find.bar.history", { shortcut: formatForKeymap("Alt+Down") })}
              aria-label={t("find.bar.recentReplace")}
              aria-haspopup="listbox"
              aria-expanded={history() === "replace"}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => setHistory(history() === "replace" ? null : "replace")}
            >
              <ReplaceAll size={14} aria-hidden="true" />
              <ChevronDown size={10} aria-hidden="true" />
            </button>
            <textarea
              ref={replaceField}
              class="fb-input"
              rows={s.multiline() ? 3 : 1}
              value={opts().replace}
              spellcheck={false}
              autocomplete="off"
              aria-label={t("find.bar.replaceWith")}
              data-own-keys={OWN_KEYS}
              onInput={(e) => set({ replace: e.currentTarget.value })}
              onKeyDown={(e) => onFieldKey(e, "replace")}
            />
            <button
              type="button"
              class="find-toggle"
              classList={{ on: s.preserveCase() }}
              title={t("find.bar.preserveCase")}
              aria-label={t("find.bar.preserveCase")}
              aria-pressed={s.preserveCase()}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => s.setPreserveCase(!s.preserveCase())}
            >
              Aa
            </button>
            <Show when={history() === "replace"}>
              <HistoryList
                id={`${id}-rhistory`}
                items={recentReplace()}
                onPick={(text) => {
                  setHistory(null);
                  set({ replace: text });
                  replaceField?.focus();
                }}
                onClose={() => {
                  setHistory(null);
                  replaceField?.focus();
                }}
              />
            </Show>
          </div>
          <button type="button" class="fb-button" disabled={!s.counter().total} onMouseDown={(e) => e.preventDefault()} onClick={() => replaceCurrent(view)}>
            {t("find.bar.replace")}
          </button>
          <button type="button" class="fb-button" disabled={!s.counter().total} onMouseDown={(e) => e.preventDefault()} onClick={() => replaceAllMatches(view)}>
            {t("find.bar.replaceAll")}
          </button>
          <button type="button" class="fb-button" disabled={!s.counter().total} onMouseDown={(e) => e.preventDefault()} onClick={() => excludeCurrent(view)}>
            {t("find.bar.exclude")}
          </button>
        </div>
      </Show>
    </div>
  );
}

/** Reactive state of one open bar, fed by the panel's `update`. */
interface BarState {
  options: () => FindOptions;
  counter: () => Counter;
  inSelection: () => boolean;
  /** A non-empty selection that "In Selection" can take as its scope. */
  hasSelection: () => boolean;
  replaceOpen: () => boolean;
  setReplaceOpen: (open: boolean) => void;
  multiline: () => boolean;
  setMultiline: (on: boolean) => void;
  preserveCase: () => boolean;
  setPreserveCase: (on: boolean) => void;
  focus: () => void;
}

function createFindPanel(view: EditorView): Panel {
  const dom = document.createElement("div");
  dom.className = "cm-find-bar";
  const query = getSearchQuery(view.state);
  const [options, setOptions] = createSignal(optionsOf(query));
  const [counter, setCounter] = createSignal<Counter>(countMatches(query, view.state));
  const [inSelection, setInSelection] = createSignal(!!view.state.field(findScopeField, false)?.ranges);
  const hasSel = () => view.state.selection.ranges.some((r) => !r.empty);
  const [hasSelection, setHasSelection] = createSignal(hasSel());
  const [replaceOpen, setReplaceOpen] = createSignal(false);
  const [multiline, setMultiline] = createSignal(query.search.includes("\n"));
  const state: BarState = {
    options,
    counter,
    inSelection,
    hasSelection,
    replaceOpen,
    setReplaceOpen,
    multiline,
    setMultiline,
    preserveCase: preserveCaseOn,
    setPreserveCase,
    focus: () => undefined,
  };
  const dispose = render(() => <FindBar view={view} state={state} />, dom);
  controllers.set(view, {
    setReplaceOpen,
    replaceOpen,
    focus: () => state.focus(),
    anchor: view.state.selection.main.from,
  });
  const recount = (u: ViewUpdate | null) => {
    const st = u?.state ?? view.state;
    setCounter(countMatches(getSearchQuery(st), st));
    setInSelection(!!st.field(findScopeField, false)?.ranges);
    setHasSelection(hasSel());
  };
  return {
    dom,
    top: true,
    mount: () => state.focus(),
    update(u) {
      let changed = u.docChanged || u.selectionSet;
      for (const tr of u.transactions)
        for (const e of tr.effects) {
          if (e.is(setSearchQuery)) {
            setOptions(optionsOf(e.value));
            if (e.value.search.includes("\n")) setMultiline(true);
            changed = true;
          } else if (e.is(setFindScope) || e.is(excludeMatch) || e.is(clearExcluded)) changed = true;
        }
      if (changed) recount(u);
    },
    destroy() {
      controllers.delete(view);
      dispose();
    },
  };
}

const scopeMark = Decoration.mark({ class: "cm-findScope" });

/** The Find/Replace bar: CodeMirror's search state with Forge's panel at the top. */
export function findBar() {
  return [
    search({ top: true, createPanel: createFindPanel }),
    findScopeField,
    EditorView.decorations.compute([findScopeField], (st) => {
      const ranges = st.field(findScopeField).ranges;
      return ranges?.length ? Decoration.set(ranges.filter((r) => r.to > r.from).map((r) => scopeMark.range(r.from, r.to)), true) : Decoration.none;
    }),
  ];
}
