/**
 * Logic of the per-file Find/Replace bar (PHPStorm's editor search): the query with its
 * "In Selection" scope and excluded matches, the match counter, replacement text with
 * Preserve Case, the prefill from the selection or the word at the caret, and the history.
 * Per-keystroke editor work, so it stays in TypeScript next to CodeMirror.
 */
import { EditorState, StateEffect, StateField, type Transaction } from "@codemirror/state";
import { SearchQuery } from "@codemirror/search";

export interface FindOptions {
  search: string;
  replace: string;
  caseSensitive: boolean;
  wholeWord: boolean;
  regexp: boolean;
}

export interface Range {
  from: number;
  to: number;
}

/** The "In Selection" ranges (null: whole file) and the matches excluded with Exclude. */
export interface FindScope {
  ranges: Range[] | null;
  /** Start positions of excluded matches. */
  excluded: number[];
}

export const setFindScope = StateEffect.define<Range[] | null>();
export const excludeMatch = StateEffect.define<number>();
export const clearExcluded = StateEffect.define<null>();

const mapScope = (scope: FindScope, tr: Transaction): FindScope =>
  tr.docChanged
    ? {
        ranges: scope.ranges?.map((r) => ({ from: tr.changes.mapPos(r.from, 1), to: tr.changes.mapPos(r.to, -1) })) ?? null,
        excluded: scope.excluded.map((p) => tr.changes.mapPos(p, 1)),
      }
    : scope;

export const findScopeField = StateField.define<FindScope>({
  create: () => ({ ranges: null, excluded: [] }),
  update(scope, tr) {
    let next = mapScope(scope, tr);
    for (const e of tr.effects) {
      if (e.is(setFindScope)) next = { ranges: e.value, excluded: [] };
      else if (e.is(excludeMatch)) next = { ...next, excluded: [...next.excluded, e.value] };
      else if (e.is(clearExcluded)) next = { ...next, excluded: [] };
    }
    return next;
  },
});

/** Filter of the search query: inside the "In Selection" ranges and not excluded. */
export function scopeTest(_match: string, state: EditorState, from: number, to: number): boolean {
  const scope = state.field(findScopeField, false);
  if (!scope) return true;
  if (scope.excluded.includes(from)) return false;
  return !scope.ranges || scope.ranges.some((r) => from >= r.from && to <= r.to);
}

/** The CodeMirror query for the bar's options. Plain text is literal, as in PHPStorm. */
export function makeQuery(o: FindOptions): SearchQuery {
  return new SearchQuery({
    search: o.search,
    replace: o.replace,
    caseSensitive: o.caseSensitive,
    wholeWord: o.wholeWord,
    regexp: o.regexp,
    literal: !o.regexp,
    test: scopeTest,
  });
}

export const optionsOf = (q: SearchQuery): FindOptions => ({
  search: q.search,
  replace: q.replace,
  caseSensitive: q.caseSensitive,
  wholeWord: q.wholeWord,
  regexp: q.regexp,
});

/** The JavaScript error of an invalid regular expression, else null. */
export function regexError(o: Pick<FindOptions, "search" | "regexp">): string | null {
  if (!o.regexp || !o.search) return null;
  try {
    new RegExp(o.search, "u");
    return null;
  } catch (e) {
    try {
      // CodeMirror compiles without the `u` flag; only report what it rejects too.
      new RegExp(o.search);
      return null;
    } catch {
      return e instanceof Error ? e.message : String(e);
    }
  }
}

export interface Match {
  from: number;
  to: number;
  /** Groups of a regex match. */
  match?: RegExpExecArray;
}

/** The matches of `query` in `state` (at most `limit`). */
export function matchesOf(query: SearchQuery, state: EditorState, limit = 10000): { matches: Match[]; capped: boolean } {
  const matches: Match[] = [];
  if (!query.valid) return { matches, capped: false };
  const cursor = query.getCursor(state) as Iterator<Match>;
  for (let r = cursor.next(); !r.done; r = cursor.next()) {
    if (matches.length >= limit) return { matches, capped: true };
    matches.push(r.value);
  }
  return { matches, capped: false };
}

export interface Counter {
  total: number;
  /** 1-based index of the match the main selection covers, 0 when it covers none. */
  current: number;
  capped: boolean;
}

/** The `n/m` counter of the bar. */
export function countMatches(query: SearchQuery, state: EditorState, limit = 10000): Counter {
  const { matches, capped } = matchesOf(query, state, limit);
  const sel = state.selection.main;
  const i = matches.findIndex((m) => m.from === sel.from && m.to === sel.to);
  return { total: matches.length, current: i + 1, capped };
}

const unquote = (text: string) =>
  text.replace(/\\([nrt\\])/g, (_, ch) => (ch === "n" ? "\n" : ch === "r" ? "\r" : ch === "t" ? "\t" : "\\"));

/** The replacement of a regex match: `$&`, `$n` and `$$`, `\n` / `\t` escapes. */
export function expandReplacement(replace: string, match: RegExpExecArray | undefined, regexp: boolean): string {
  if (!regexp || !match) return replace;
  return unquote(replace).replace(/\$([$&]|\d+)/g, (m, i: string) => {
    if (i === "&") return match[0];
    if (i === "$") return "$";
    for (let l = i.length; l > 0; l--) {
      const n = +i.slice(0, l);
      if (n > 0 && n < match.length) return (match[n] ?? "") + i.slice(l);
    }
    return m;
  });
}

const hasLetters = (s: string) => s.toLowerCase() !== s.toUpperCase();

/**
 * Preserve Case: an upper-case match gets an upper-case replacement, a capitalized match a
 * capitalized one, a lower-case match a lower-case one (PHPStorm).
 */
export function preserveCase(found: string, replacement: string): string {
  if (!hasLetters(found) || !replacement) return replacement;
  if (found.length > 1 && found === found.toUpperCase()) return replacement.toUpperCase();
  const first = found.charAt(0);
  if (first === first.toUpperCase() && hasLetters(first)) return replacement.charAt(0).toUpperCase() + replacement.slice(1);
  if (found === found.toLowerCase()) return replacement.charAt(0).toLowerCase() + replacement.slice(1);
  return replacement;
}

/** The text that replaces `m`. */
export function replacementFor(state: EditorState, query: SearchQuery, m: Match, keepCase: boolean): string {
  const text = expandReplacement(query.replace, m.match, query.regexp);
  return keepCase ? preserveCase(state.sliceDoc(m.from, m.to), text) : text;
}

export interface Prefill {
  /** The search text to fill in; undefined keeps the current one. */
  search?: string;
  /** A multi-line selection turns "In Selection" on with these ranges. */
  scope: Range[] | null;
}

/**
 * What Find / Replace start with: a single-line selection, or the word at the caret; a
 * selection over several lines becomes the "In Selection" scope instead.
 */
export function prefillFor(state: EditorState): Prefill {
  const sel = state.selection.main;
  if (!sel.empty) {
    const text = state.sliceDoc(sel.from, sel.to);
    if (text.includes("\n")) {
      const ranges = state.selection.ranges.filter((r) => !r.empty).map((r) => ({ from: r.from, to: r.to }));
      return { scope: ranges };
    }
    return { search: text, scope: null };
  }
  const word = state.wordAt(sel.head);
  return { search: word ? state.sliceDoc(word.from, word.to) : undefined, scope: null };
}

/** Put `entry` first in a history list without duplicates, keeping at most `max` entries. */
export function pushHistory(list: readonly string[], entry: string, max = 20): string[] {
  if (!entry) return [...list];
  return [entry, ...list.filter((e) => e !== entry)].slice(0, max);
}
