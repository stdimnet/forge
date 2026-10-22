import { describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView, runScopeHandlers } from "@codemirror/view";
import { editingApi } from "@/ipc/editing";
import "@/mock/editingMock";
import { applyPostfix, loadPostfixTemplates, postfixCompletion, postfixFamily } from "./postfix";
import { completeStatement } from "./statement";
import { expandWord } from "./hippie";

/** A view with the caret at `|`. */
function viewOf(doc: string, extensions = postfixCompletion("php") ?? []): EditorView {
  const pos = doc.indexOf("|");
  return new EditorView({
    state: EditorState.create({ doc: doc.replace("|", ""), selection: { anchor: pos }, extensions }),
  });
}

/** The document with `|` at the caret, or `[…]` around the selection. */
function shown(view: EditorView): string {
  const { from, to } = view.state.selection.main;
  const doc = view.state.doc.toString();
  return from === to ? `${doc.slice(0, from)}|${doc.slice(to)}` : `${doc.slice(0, from)}[${doc.slice(from, to)}]${doc.slice(to)}`;
}

// jsdom has no layout; CodeMirror measures text ranges after updates.
const noRects = () => Object.assign([], { item: () => null }) as unknown as DOMRectList;
Range.prototype.getClientRects ??= noRects;
Range.prototype.getBoundingClientRect ??= () => new DOMRect();

const tick = () => new Promise((r) => setTimeout(r, 0));

describe("postfix completion", () => {
  it("lists the templates from Rust", async () => {
    const all = await loadPostfixTemplates();
    expect(all.find((t) => t.id === "php.if")).toMatchObject({ group: "PHP", before: "$expr.if" });
    expect(new Set(all.map((t) => t.language))).toEqual(new Set(["php", "js", "lua"]));
    expect(postfixFamily("tsx")).toBe("js");
    expect(postfixCompletion("markdown")).toBeNull();
  });

  it("applies a template from the completion list", async () => {
    const view = viewOf("<?php\n    $user->isAdmin().i|");
    const line = view.state.doc.line(2);
    const result = await editingApi.postfixComplete({ language: "php", line: line.text, indent: "    " });
    const item = result!.items.find((i) => i.key === "if")!;
    applyPostfix(view, line.from, item, view.state.selection.main.head);
    expect(shown(view)).toBe("<?php\n    if ($user->isAdmin()) {\n        |\n    }");
  });

  it("expands a full key with Tab", async () => {
    await loadPostfixTemplates();
    const view = viewOf("$items.foreach|");
    expect(runScopeHandlers(view, new KeyboardEvent("keydown", { key: "Tab" }), "editor")).toBe(true);
    await tick();
    await tick();
    expect(shown(view)).toBe("foreach ($items as [$item]) {\n  \n}");
  });

  it("leaves Tab alone without a template key", () => {
    const view = viewOf("$items.nothing|");
    expect(runScopeHandlers(view, new KeyboardEvent("keydown", { key: "Tab" }), "editor")).toBe(false);
  });
});

describe("Complete Current Statement", () => {
  it("closes brackets and adds the semicolon", async () => {
    const view = viewOf("$a = foo(bar(1|", []);
    await completeStatement(view, "php");
    expect(shown(view)).toBe("$a = foo(bar(1));|");
  });

  it("opens a Lua block", async () => {
    const view = viewOf("if ready|", []);
    await completeStatement(view, "lua");
    expect(shown(view)).toBe("if ready then\n  |\nend");
  });

  it("starts a new line in other languages", async () => {
    const view = viewOf("text|", []);
    await completeStatement(view, "markdown");
    expect(shown(view)).toBe("text\n|");
  });
});

describe("Cyclic Expand Word", () => {
  it("expands and cycles through the words", async () => {
    const view = viewOf("counter country\ncou|", []);
    const others = () => ["county"];
    await expandWord(view, { backward: false, others });
    expect(shown(view)).toBe("counter country\ncountry|");
    await expandWord(view, { backward: false, others });
    expect(shown(view)).toBe("counter country\ncounter|");
    await expandWord(view, { backward: false, others });
    expect(shown(view)).toBe("counter country\ncounty|");
    await expandWord(view, { backward: true, others });
    expect(shown(view)).toBe("counter country\ncounter|");
  });

  it("does nothing without matching words", async () => {
    const view = viewOf("zzz|", []);
    await expandWord(view, { backward: false, others: () => [] });
    expect(shown(view)).toBe("zzz|");
  });
});
