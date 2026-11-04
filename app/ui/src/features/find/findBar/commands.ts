/**
 * Editor commands of the Find/Replace bar: open it for Find (collapsed) or Replace
 * (expanded), replace the current match or all of them, exclude a match, close the bar.
 */
import { EditorSelection, type StateEffect } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import {
  closeSearchPanel,
  findNext,
  getSearchQuery,
  openSearchPanel,
  searchPanelOpen,
  selectMatches,
  setSearchQuery,
} from "@codemirror/search";
import { getSetting, saveSetting } from "@/state/settings";
import { announce } from "@/features/accessibility/announce";
import { t } from "@/i18n";
import {
  clearExcluded,
  excludeMatch,
  findScopeField,
  makeQuery,
  matchesOf,
  optionsOf,
  prefillFor,
  pushHistory,
  replacementFor,
  setFindScope,
  type FindOptions,
  type Match,
} from "./logic";

/** Settings that remember the bar's options, like PHPStorm's FindSettings. */
export const FIND_SETTINGS = {
  matchCase: "editor.find.matchCase",
  words: "editor.find.words",
  regex: "editor.find.regex",
  preserveCase: "editor.find.preserveCase",
  recentFind: "editor.find.recentFind",
  recentReplace: "editor.find.recentReplace",
} as const;

const save = (key: string, value: boolean | string[]) => void saveSetting(key, value).catch(() => undefined);

/** The options the bar starts with when it opens. */
export const savedOptions = (): Pick<FindOptions, "caseSensitive" | "wholeWord" | "regexp"> => ({
  caseSensitive: getSetting<boolean>(FIND_SETTINGS.matchCase, false),
  wholeWord: getSetting<boolean>(FIND_SETTINGS.words, false),
  regexp: getSetting<boolean>(FIND_SETTINGS.regex, false),
});

export function rememberOptions(o: Pick<FindOptions, "caseSensitive" | "wholeWord" | "regexp">): void {
  const s = savedOptions();
  if (s.caseSensitive !== o.caseSensitive) save(FIND_SETTINGS.matchCase, o.caseSensitive);
  if (s.wholeWord !== o.wholeWord) save(FIND_SETTINGS.words, o.wholeWord);
  if (s.regexp !== o.regexp) save(FIND_SETTINGS.regex, o.regexp);
}

export const preserveCaseOn = () => getSetting<boolean>(FIND_SETTINGS.preserveCase, false);
export const setPreserveCase = (on: boolean) => save(FIND_SETTINGS.preserveCase, on);

export const recentFind = () => getSetting<string[]>(FIND_SETTINGS.recentFind, []);
export const recentReplace = () => getSetting<string[]>(FIND_SETTINGS.recentReplace, []);

/** Remember the current search (and replace) text in the history. */
export function rememberHistory(view: EditorView, withReplace: boolean): void {
  const q = getSearchQuery(view.state);
  if (q.search && recentFind()[0] !== q.search) save(FIND_SETTINGS.recentFind, pushHistory(recentFind(), q.search));
  if (withReplace && q.replace && recentReplace()[0] !== q.replace) save(FIND_SETTINGS.recentReplace, pushHistory(recentReplace(), q.replace));
}

/** What the open panel of a view exposes to the commands. */
export interface FindBarController {
  setReplaceOpen(open: boolean): void;
  replaceOpen(): boolean;
  focus(): void;
  /** Where incremental search starts (the caret when the bar opened). */
  anchor: number;
}

export const controllers = new WeakMap<EditorView, FindBarController>();

/**
 * Find (Ctrl+F) / Replace (Ctrl+R): open the bar at the top of the editor, prefilled from the
 * selection or the word at the caret; a selection over several lines turns on "In Selection".
 */
export function openFindBar(view: EditorView, replace: boolean): boolean {
  const open = searchPanelOpen(view.state);
  const prefill = prefillFor(view.state);
  const current = optionsOf(getSearchQuery(view.state));
  const base: FindOptions = open ? current : { ...current, ...savedOptions() };
  // With the bar open, an empty caret keeps the text the user typed.
  const search = open && view.state.selection.main.empty ? base.search : (prefill.search ?? base.search);
  if (!open) openSearchPanel(view);
  const effects: StateEffect<unknown>[] = [setSearchQuery.of(makeQuery({ ...base, search }))];
  if (prefill.scope || !open) effects.push(setFindScope.of(prefill.scope));
  view.dispatch({ effects });
  const c = controllers.get(view);
  if (c) {
    c.anchor = view.state.selection.main.from;
    c.setReplaceOpen(replace);
    c.focus();
  }
  return true;
}

export const openFind = (view: EditorView) => openFindBar(view, false);
export const openReplace = (view: EditorView) => openFindBar(view, true);

/** Close the bar and give the focus back to the editor. */
export function closeFindBar(view: EditorView): void {
  rememberHistory(view, controllers.get(view)?.replaceOpen() ?? false);
  closeSearchPanel(view);
  if (view.state.field(findScopeField, false)?.ranges) view.dispatch({ effects: setFindScope.of(null) });
  view.focus();
}

/** The match the main selection covers, if any. */
function currentMatch(view: EditorView): Match | undefined {
  const q = getSearchQuery(view.state);
  if (!q.valid) return undefined;
  const sel = view.state.selection.main;
  const cursor = q.getCursor(view.state, sel.from) as Iterator<Match>;
  const first = cursor.next();
  return !first.done && first.value.from === sel.from && first.value.to === sel.to ? first.value : undefined;
}

/** Replace: the current match, then select the next one. */
export function replaceCurrent(view: EditorView): boolean {
  const q = getSearchQuery(view.state);
  if (!q.valid || view.state.readOnly) return false;
  const m = currentMatch(view);
  if (m) {
    const insert = replacementFor(view.state, q, m, preserveCaseOn());
    view.dispatch({
      changes: { from: m.from, to: m.to, insert },
      selection: EditorSelection.cursor(m.from + insert.length),
      userEvent: "input.replace",
    });
  }
  rememberHistory(view, true);
  return findNext(view);
}

/** Replace All: every match (in the selection scope, without excluded ones). */
export function replaceAllMatches(view: EditorView): number {
  const q = getSearchQuery(view.state);
  if (!q.valid || view.state.readOnly) return 0;
  const { matches } = matchesOf(q, view.state, Number.MAX_SAFE_INTEGER);
  if (!matches.length) return 0;
  const keep = preserveCaseOn();
  view.dispatch({
    changes: matches.map((m) => ({ from: m.from, to: m.to, insert: replacementFor(view.state, q, m, keep) })),
    userEvent: "input.replace.all",
  });
  rememberHistory(view, true);
  announce(t("find.bar.replaced", { count: matches.length }));
  return matches.length;
}

/** Exclude: leave the current match out of Replace All and the counter, go to the next one. */
export function excludeCurrent(view: EditorView): boolean {
  const m = currentMatch(view);
  if (m) view.dispatch({ effects: excludeMatch.of(m.from) });
  return findNext(view);
}

/** Select All Occurrences (Alt+Enter in the bar): a caret on every match, focus to the editor. */
export function selectAllMatches(view: EditorView): boolean {
  rememberHistory(view, false);
  const ok = selectMatches(view);
  view.focus();
  return ok;
}

/** A new search text: forget excluded matches and move to the first match from the anchor. */
export function updateSearch(view: EditorView, o: FindOptions, incremental: boolean): void {
  const before = getSearchQuery(view.state);
  const query = makeQuery(o);
  const effects: StateEffect<unknown>[] = [setSearchQuery.of(query)];
  if (before.search !== o.search || before.regexp !== o.regexp) effects.push(clearExcluded.of(null));
  view.dispatch({ effects });
  if (!incremental || !query.valid) return;
  const anchor = controllers.get(view)?.anchor ?? view.state.selection.main.from;
  const after = (query.getCursor(view.state, anchor) as Iterator<Match>).next();
  const hit = !after.done ? after.value : (query.getCursor(view.state) as Iterator<Match>).next().value;
  if (hit) view.dispatch({ selection: EditorSelection.single(hit.from, hit.to), scrollIntoView: true });
}
