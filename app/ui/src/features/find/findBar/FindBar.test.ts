import { afterEach, describe, expect, it } from "vitest";
import "@/mock/settingsMock";
import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { getSearchQuery, searchPanelOpen } from "@codemirror/search";
import { findBar } from "./FindBar";
import { closeFindBar, excludeCurrent, openFindBar, replaceAllMatches, replaceCurrent } from "./commands";
import { findScopeField } from "./logic";

let view: EditorView | undefined;
afterEach(() => {
  view?.destroy();
  view = undefined;
  document.body.innerHTML = "";
});

function editor(doc: string, selection?: EditorSelection) {
  view = new EditorView({
    parent: document.body,
    state: EditorState.create({ doc, selection, extensions: [findBar(), EditorState.allowMultipleSelections.of(true)] }),
  });
  return view;
}

const bar = (v: EditorView) => v.dom.querySelector(".cm-panels-top .cm-find-bar");
const status = (v: EditorView) => v.dom.querySelector(".fb-status")?.textContent;
const type = (v: EditorView, text: string) => {
  const field = v.dom.querySelector<HTMLTextAreaElement>(".cm-find-bar textarea")!;
  field.value = text;
  field.dispatchEvent(new Event("input", { bubbles: true }));
};

describe("Find/Replace bar", () => {
  it("opens at the top, collapsed for Find and expanded for Replace", () => {
    const v = editor("alpha beta alpha", EditorSelection.single(1));
    openFindBar(v, false);
    expect(bar(v)).toBeTruthy();
    expect(v.dom.querySelectorAll(".cm-find-bar .fb-row")).toHaveLength(1);
    expect(getSearchQuery(v.state).search).toBe("alpha");
    expect(document.activeElement).toBe(v.dom.querySelector(".cm-find-bar textarea"));
    openFindBar(v, true);
    expect(v.dom.querySelectorAll(".cm-find-bar .fb-row")).toHaveLength(2);
    expect(v.dom.querySelector(".fb-expand")?.getAttribute("aria-expanded")).toBe("true");
  });

  it("counts matches and marks a search without results", () => {
    const v = editor("foo bar foo");
    openFindBar(v, false);
    type(v, "foo");
    // Incremental search selects the first match from the caret.
    expect(status(v)).toBe("1/2");
    type(v, "nothing");
    expect(status(v)).toBe("0 results");
    expect(v.dom.querySelector(".fb-field")?.classList.contains("error")).toBe(true);
  });

  it("turns In Selection on for a selection over several lines", () => {
    const v = editor("x\nx\nx\nx", EditorSelection.single(2, 5));
    openFindBar(v, false);
    expect(v.state.field(findScopeField).ranges).toEqual([{ from: 2, to: 5 }]);
    type(v, "x");
    expect(status(v)).toMatch(/\/2$/);
    closeFindBar(v);
    expect(searchPanelOpen(v.state)).toBe(false);
    expect(v.state.field(findScopeField).ranges).toBeNull();
  });

  it("replaces, excludes and replaces all", () => {
    const v = editor("cat cat cat cat");
    openFindBar(v, true);
    type(v, "cat");
    const replace = v.dom.querySelectorAll<HTMLTextAreaElement>(".cm-find-bar textarea")[1];
    replace.value = "dog";
    replace.dispatchEvent(new Event("input", { bubbles: true }));
    replaceCurrent(v);
    expect(v.state.doc.toString()).toBe("dog cat cat cat");
    excludeCurrent(v);
    expect(replaceAllMatches(v)).toBe(2);
    expect(v.state.doc.toString()).toBe("dog cat dog dog");
  });
});
