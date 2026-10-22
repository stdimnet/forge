import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { history, undo, undoDepth } from "@codemirror/commands";
import { registerMock } from "@/ipc";
import type { EditPlan, WorkspaceEdit } from "@/ipc/codeintel";
import type { IntroduceInfo, RefactorAnswer } from "@/ipc/refactor";
import { commands } from "@/state/commands";
import { registerView } from "@/editor/views";
import { refactoringHistory } from "./refactoring";
import { commitRename, currentName, inPlaceRename, renameSessionOf } from "./rename";
import { chooser, inline, inlineDialog, introduce, refactorHighlight } from "./extract";
import "./index";

const noRects = () => Object.assign([], { item: () => null }) as unknown as DOMRectList;
Range.prototype.getClientRects ??= noRects;
Range.prototype.getBoundingClientRect ??= () => new DOMRect();

const views: EditorView[] = [];
const unregister: (() => void)[] = [];
function make(path: string, doc: string): EditorView {
  const view = new EditorView({
    state: EditorState.create({
      doc,
      extensions: [history(), refactoringHistory(), inPlaceRename(), refactorHighlight()],
    }),
  });
  views.push(view);
  unregister.push(registerView(path, view));
  return view;
}
afterEach(() => {
  unregister.splice(0).forEach((u) => u());
  views.splice(0).forEach((v) => v.destroy());
});

const PATH = "app/Cart.php";
const CART = "<?php\nfunction total($a, $b)\n{\n    return $a * $b + 1;\n}\n";
const range = (line: number, from: number, to: number) => ({
  start: { line, character: from },
  end: { line, character: to },
});
const tick = () => new Promise((r) => setTimeout(r));

const info = (over: Partial<IntroduceInfo> = {}): IntroduceInfo => ({
  range: range(3, 11, 18),
  text: "$a * $b",
  occurrences: 1,
  names: ["result", "product"],
  initPlaces: [],
  isStatic: false,
  constantExpression: false,
  global: false,
  callSites: 0,
  ...over,
});

/** What Rust answers for `$<name> = $a * $b;` before the return. */
function introduceAnswer(name: string): RefactorAnswer {
  const edit: WorkspaceEdit = {
    changes: [
      {
        kind: "edit",
        path: PATH,
        edits: [
          { range: range(3, 4, 4), newText: `$${name} = $a * $b;\n    ` },
          { range: range(3, 11, 18), newText: `$${name}` },
        ],
      },
    ],
  };
  return {
    edit,
    preview: [],
    conflicts: [],
    nameRanges: [range(3, 5, 5 + name.length), range(4, 12, 12 + name.length)],
    skipped: 0,
  };
}

beforeEach(() => {
  registerMock("refactor_introduce_prepare", () => ({
    expressions: [],
    info: info(),
  }));
  registerMock("refactor_introduce", ({ options }) => introduceAnswer((options as { name: string }).name));
  registerMock("code_plan_edit", ({ edit }): EditPlan => {
    const e = edit as WorkspaceEdit;
    return {
      editors: e.changes.flatMap((c) => (c.kind === "edit" ? [{ path: c.path, edits: c.edits }] : [])),
      backend: { changes: [] },
      saveFirst: [],
      reopen: [],
      close: [],
      fileCount: 1,
    };
  });
});

describe("Extract / Introduce and Inline wiring", () => {
  it("implements PHPStorm's actions, so the shortcuts, menus and Refactor This work", () => {
    for (const id of [
      "IntroduceVariable",
      "IntroduceConstant",
      "IntroduceField",
      "IntroduceParameter",
      "ExtractMethod",
      "Inline",
      "Refactorings.QuickListPopupAction",
    ]) {
      expect(commands().has(id), id).toBe(true);
    }
  });
});

describe("Introduce Variable", () => {
  it("edits the suggested name in place and applies the final name as one undo step", async () => {
    const view = make(PATH, CART);
    view.dispatch({ selection: { anchor: 42, head: 49 } });
    await introduce("variable", PATH, view);
    const session = renameSessionOf(view);
    expect(session?.local?.suggestions).toEqual(["result", "product"]);
    expect(currentName(view)).toBe("result");
    expect(view.state.doc.toString()).toBe("<?php\nfunction total($a, $b)\n{\n    $result = $a * $b;\n    return $result + 1;\n}\n");
    // The preview is not an undo step.
    expect(undoDepth(view.state)).toBe(0);
    // Typing a name edits every occurrence.
    const p = session!.ranges[session!.primary];
    view.dispatch({
      changes: { from: p.from, to: p.to, insert: "area" },
      userEvent: "input.type",
    });
    expect(view.state.doc.toString()).toContain("$area = $a * $b;\n    return $area + 1;");
    await commitRename(view);
    expect(renameSessionOf(view)).toBeNull();
    expect(view.state.doc.toString()).toBe("<?php\nfunction total($a, $b)\n{\n    $area = $a * $b;\n    return $area + 1;\n}\n");
    expect(undoDepth(view.state)).toBe(1);
    undo(view);
    await tick();
    expect(view.state.doc.toString()).toBe(CART);
  });

  it("asks which expression when the caret is in several", async () => {
    registerMock("refactor_introduce_prepare", ({ start, end }) =>
      (start as { character: number }).character === (end as { character: number }).character
        ? {
            expressions: [
              { range: range(3, 11, 13), text: "$a" },
              { range: range(3, 11, 18), text: "$a * $b" },
            ],
          }
        : { expressions: [], info: info() },
    );
    const view = make(PATH, CART);
    view.dispatch({ selection: { anchor: 43 } });
    const running = introduce("variable", PATH, view);
    await tick();
    await tick();
    const popup = chooser();
    expect(popup?.title).toBe("Expressions");
    expect(popup?.items.map((i) => i.label)).toEqual(["$a", "$a * $b"]);
    popup!.done(1);
    await running;
    expect(currentName(view)).toBe("result");
  });
});

describe("Inline", () => {
  it("inlines a variable used once without asking", async () => {
    registerMock("refactor_inline_prepare", () => ({
      kind: "variable",
      title: "Inline Variable",
      name: "$x",
      onReference: true,
      occurrences: 1,
      readOnly: false,
    }));
    registerMock(
      "refactor_inline",
      (): RefactorAnswer => ({
        edit: {
          changes: [
            {
              kind: "edit",
              path: PATH,
              edits: [{ range: range(3, 11, 13), newText: "2" }],
            },
          ],
        },
        preview: [],
        conflicts: [],
        nameRanges: [],
        skipped: 0,
      }),
    );
    const view = make(PATH, CART);
    view.dispatch({ selection: { anchor: 43 } });
    await inline(PATH, view);
    expect(inlineDialog()).toBeNull();
    expect(view.state.doc.toString()).toContain("return 2 * $b + 1;");
  });

  it("asks how much to inline otherwise", async () => {
    registerMock("refactor_inline_prepare", () => ({
      kind: "method",
      title: "Inline Method",
      name: "total()",
      onReference: true,
      occurrences: 3,
      readOnly: false,
    }));
    const view = make(PATH, CART);
    const running = inline(PATH, view);
    await tick();
    await tick();
    expect(inlineDialog()?.info.name).toBe("total()");
    inlineDialog()!.done(null);
    await running;
    expect(view.state.doc.toString()).toBe(CART);
  });
});
