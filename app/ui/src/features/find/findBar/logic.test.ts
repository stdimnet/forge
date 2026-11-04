import { describe, expect, it } from "vitest";
import { EditorSelection, EditorState } from "@codemirror/state";
import {
  countMatches,
  excludeMatch,
  expandReplacement,
  findScopeField,
  makeQuery,
  matchesOf,
  prefillFor,
  preserveCase,
  pushHistory,
  regexError,
  replacementFor,
  setFindScope,
  type FindOptions,
} from "./logic";

const opts = (search: string, patch: Partial<FindOptions> = {}): FindOptions => ({
  search,
  replace: "",
  caseSensitive: false,
  wholeWord: false,
  regexp: false,
  ...patch,
});

const stateOf = (doc: string, selection?: EditorSelection) =>
  EditorState.create({ doc, selection, extensions: [findScopeField, EditorState.allowMultipleSelections.of(true)] });

describe("counter", () => {
  it("counts matches and the one the selection covers", () => {
    const st = stateOf("foo bar foo baz foo", EditorSelection.single(8, 11));
    expect(countMatches(makeQuery(opts("foo")), st)).toEqual({ total: 3, current: 2, capped: false });
  });

  it("reports 0 for a caret outside the matches and caps large counts", () => {
    const st = stateOf("aaaa");
    expect(countMatches(makeQuery(opts("a")), st).current).toBe(0);
    expect(countMatches(makeQuery(opts("a")), st, 2)).toEqual({ total: 2, current: 0, capped: true });
  });

  it("follows Match Case, Words and Regex", () => {
    const st = stateOf("Foo foo foobar");
    expect(countMatches(makeQuery(opts("foo", { caseSensitive: true })), st).total).toBe(2);
    expect(countMatches(makeQuery(opts("foo", { wholeWord: true })), st).total).toBe(2);
    expect(countMatches(makeQuery(opts("fo+b", { regexp: true })), st).total).toBe(1);
  });

  it("treats plain text literally and reports bad regular expressions", () => {
    const st = stateOf("a\\nb");
    expect(countMatches(makeQuery(opts("\\n")), st).total).toBe(1);
    expect(regexError({ search: "(foo", regexp: true })).toBeTruthy();
    expect(regexError({ search: "(foo", regexp: false })).toBeNull();
    expect(makeQuery(opts("(foo", { regexp: true })).valid).toBe(false);
  });
});

describe("in selection and exclude", () => {
  it("finds only inside the scope, mapped through edits", () => {
    let st = stateOf("x\nx\nx\nx");
    st = st.update({ effects: setFindScope.of([{ from: 2, to: 5 }]) }).state;
    const q = makeQuery(opts("x"));
    expect(matchesOf(q, st).matches.map((m) => m.from)).toEqual([2, 4]);
    st = st.update({ changes: { from: 0, insert: "yy" } }).state;
    expect(matchesOf(q, st).matches.map((m) => m.from)).toEqual([4, 6]);
    st = st.update({ effects: setFindScope.of(null) }).state;
    expect(matchesOf(q, st).matches).toHaveLength(4);
  });

  it("leaves excluded matches out", () => {
    let st = stateOf("a a a");
    st = st.update({ effects: excludeMatch.of(2) }).state;
    expect(matchesOf(makeQuery(opts("a")), st).matches.map((m) => m.from)).toEqual([0, 4]);
  });
});

describe("replacement", () => {
  it("preserves case like PHPStorm", () => {
    expect(preserveCase("FOO", "bar")).toBe("BAR");
    expect(preserveCase("Foo", "bar")).toBe("Bar");
    expect(preserveCase("foo", "Bar")).toBe("bar");
    expect(preserveCase("fOo", "bar")).toBe("bar");
    expect(preserveCase("123", "Bar")).toBe("Bar");
  });

  it("expands groups and escapes in regex mode only", () => {
    const m = /(\w+)@(\w+)/.exec("me@host")!;
    expect(expandReplacement("$2:$1 $& $$", m, true)).toBe("host:me me@host $");
    expect(expandReplacement("a\\tb", m, true)).toBe("a\tb");
    expect(expandReplacement("$1\\t", m, false)).toBe("$1\\t");
  });

  it("combines regex groups with Preserve Case", () => {
    const st = stateOf("Hello world");
    const q = makeQuery(opts("(h)ello", { regexp: true, replace: "$1i" }));
    const [m] = matchesOf(q, st).matches;
    expect(replacementFor(st, q, m, true)).toBe("Hi");
    expect(replacementFor(st, q, m, false)).toBe("Hi");
  });
});

describe("prefill", () => {
  it("takes a single-line selection", () => {
    expect(prefillFor(stateOf("one two", EditorSelection.single(4, 7)))).toEqual({ search: "two", scope: null });
  });

  it("takes the word at the caret", () => {
    expect(prefillFor(stateOf("one two", EditorSelection.single(5)))).toEqual({ search: "two", scope: null });
    expect(prefillFor(stateOf("a  b", EditorSelection.single(2))).search).toBeUndefined();
  });

  it("turns a multi-line selection into the In Selection scope", () => {
    expect(prefillFor(stateOf("one\ntwo\nthree", EditorSelection.single(2, 9)))).toEqual({ scope: [{ from: 2, to: 9 }] });
  });
});

describe("history", () => {
  it("puts the newest entry first without duplicates", () => {
    expect(pushHistory(["b", "a"], "a")).toEqual(["a", "b"]);
    expect(pushHistory(["a", "b", "c"], "d", 3)).toEqual(["d", "a", "b"]);
    expect(pushHistory(["a"], "")).toEqual(["a"]);
  });
});
