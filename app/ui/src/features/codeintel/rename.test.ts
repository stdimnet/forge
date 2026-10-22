import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { history, redo, undo, undoDepth } from "@codemirror/commands";
import { registerMock } from "@/ipc";
import type { EditPlan, PrepareRenameAnswer, RenameAnswer, WorkspaceEdit } from "@/ipc/codeintel";
import { commands } from "@/state/commands";
import { registerView } from "@/editor/views";
import { applyWorkspaceEdit, refactoringHistory } from "./refactoring";
import { cancelRename, commitRename, currentName, inPlaceRename, premiumHint, renameSessionOf, resetPremiumHint, startRename } from "./rename";
import { actionIcon } from "./intentions";
import { conflictsAnswer } from "./RefactoringDialogs";
import "./index";

// jsdom has no layout: CodeMirror's measure pass (it runs while the tests await) needs these.
const noRects = () => Object.assign([], { item: () => null }) as unknown as DOMRectList;
Range.prototype.getClientRects ??= noRects;
Range.prototype.getBoundingClientRect ??= () => new DOMRect();

const views: EditorView[] = [];
const unregister: (() => void)[] = [];
function make(path: string, doc: string): EditorView {
  const view = new EditorView({
    state: EditorState.create({ doc, extensions: [history(), refactoringHistory(), inPlaceRename()] }),
  });
  views.push(view);
  unregister.push(registerView(path, view));
  return view;
}
// jsdom has no layout; destroy the views before CodeMirror's measure pass runs.
afterEach(() => {
  unregister.splice(0).forEach((u) => u());
  views.splice(0).forEach((v) => v.destroy());
});

const range = (line: number, from: number, to: number) => ({ start: { line, character: from }, end: { line, character: to } });
const USER = "<?php\nnew Order();\nOrder::find(1);\n";
const CONTROLLER = "<?php\nfunction show(Order $o) {}\n";

/** The edit Rust answers for renaming `Order` in both files. */
function renameEdit(name: string): WorkspaceEdit {
  return {
    changes: [
      { kind: "edit", path: "app/User.php", edits: [range(1, 4, 9), range(2, 0, 5)].map((r) => ({ range: r, newText: name })) },
      { kind: "edit", path: "app/Ctrl.php", edits: [{ range: range(1, 14, 19), newText: name }] },
    ],
  };
}

const answer = (over: Partial<PrepareRenameAnswer> = {}): PrepareRenameAnswer => ({
  range: range(1, 4, 9),
  placeholder: "Order",
  invalid: false,
  source: "index",
  occurrences: [range(1, 4, 9), range(2, 0, 5)],
  sigil: false,
  ...over,
});

beforeEach(() => {
  resetPremiumHint();
  registerMock("code_prepare_rename", () => answer());
  registerMock("code_rename", ({ newName }): RenameAnswer => ({
    edit: renameEdit(String(newName)),
    preview: [],
    conflicts: [],
    source: "index",
    missing: { feature: "Rename", message: "Rename requires Intelephense Premium", premium: true },
    skipped: 0,
  }));
  // Both files are open: every text edit goes to its editor.
  registerMock("code_plan_edit", ({ edit }): EditPlan => {
    const e = edit as WorkspaceEdit;
    return {
      editors: e.changes.flatMap((c) => (c.kind === "edit" ? [{ path: c.path, edits: c.edits }] : [])),
      backend: { changes: [] },
      saveFirst: [],
      reopen: [],
      close: [],
      fileCount: 2,
    };
  });
  registerMock("lsp_did_change", () => undefined);
});

/** Type `text` over the selection like the keyboard does. */
function type(view: EditorView, text: string) {
  const { from, to } = view.state.selection.main;
  view.dispatch({ changes: { from, to, insert: text }, selection: { anchor: from + text.length }, userEvent: "input.type" });
}

describe("Rename and Show Context Actions wiring", () => {
  it("implements PHPStorm's actions, so Shift+F6, Alt+Enter and the main menu work", () => {
    for (const id of ["RenameElement", "RenameFile", "ShowIntentionActions"]) expect(commands().has(id), id).toBe(true);
  });

  it("picks PHPStorm's icons", () => {
    const a = { title: "x", source: "lsp", isPreferred: false, needsResolve: false };
    expect(actionIcon({ ...a, kind: "quickFix" })).toBe("fix");
    expect(actionIcon({ ...a, kind: "intention" })).toBe("intention");
    expect(actionIcon({ ...a, kind: "intention", title: "Generate...", action: "Generate" })).toBe("link");
    // Intentions that run a UI action without opening another popup keep the bulb.
    expect(actionIcon({ ...a, kind: "intention", title: "Edit SQL Fragment", action: "EditInjectedFragment" })).toBe("intention");
    expect(actionIcon({ ...a, kind: "refactor" })).toBe("refactor");
  });
});

describe("in-place rename", () => {
  it("edits every occurrence live and Escape puts the old name back", async () => {
    const view = make("app/User.php", USER);
    view.dispatch({ selection: { anchor: 8 } });
    await startRename("app/User.php", view);
    expect(renameSessionOf(view)).not.toBeNull();
    // The whole name is selected, like PHPStorm.
    expect(view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to)).toBe("Order");
    type(view, "Purch");
    type(view, "ase");
    expect(view.state.doc.toString()).toBe("<?php\nnew Purchase();\nPurchase::find(1);\n");
    expect(currentName(view)).toBe("Purchase");
    // Live edits are not undo steps.
    expect(undoDepth(view.state)).toBe(0);
    cancelRename(view);
    expect(view.state.doc.toString()).toBe(USER);
    expect(renameSessionOf(view)).toBeNull();
  });

  it("renames across open files as one undoable step and shows the premium hint once", async () => {
    const view = make("app/User.php", USER);
    const other = make("app/Ctrl.php", CONTROLLER);
    view.dispatch({ selection: { anchor: 8 } });
    await startRename("app/User.php", view);
    type(view, "Purchase");
    await commitRename(view);
    expect(view.state.doc.toString()).toBe("<?php\nnew Purchase();\nPurchase::find(1);\n");
    expect(other.state.doc.toString()).toBe("<?php\nfunction show(Purchase $o) {}\n");
    expect(renameSessionOf(view)).toBeNull();
    expect(undoDepth(view.state)).toBe(1);
    // The hint was shown for this rename; it is not repeated.
    expect(premiumHint({ source: "index", missing: { feature: "Rename", message: "", premium: true } })).toBe(false);

    // Ctrl+Z in one editor undoes the whole rename.
    undo(view);
    await new Promise((r) => setTimeout(r));
    expect(view.state.doc.toString()).toBe(USER);
    expect(other.state.doc.toString()).toBe(CONTROLLER);
    redo(view);
    await new Promise((r) => setTimeout(r));
    expect(other.state.doc.toString()).toBe("<?php\nfunction show(Purchase $o) {}\n");
  });

  it("keeps the edit box open when the name is invalid", async () => {
    registerMock("code_rename", () => {
      throw new Error("'1x' is not a valid identifier");
    });
    const view = make("app/User.php", USER);
    view.dispatch({ selection: { anchor: 8 } });
    await startRename("app/User.php", view);
    type(view, "1x");
    await commitRename(view);
    expect(renameSessionOf(view)).not.toBeNull();
    expect(view.state.doc.toString()).toBe("<?php\nnew 1x();\n1x::find(1);\n");
  });

  it("does not start where nothing can be renamed", async () => {
    registerMock("code_prepare_rename", () => answer({ invalid: true, range: null, message: "Cannot perform refactoring" }));
    const view = make("app/User.php", USER);
    await startRename("app/User.php", view);
    expect(renameSessionOf(view)).toBeNull();
  });
});

describe("refactoring undo", () => {
  it("leaves editors alone that were edited since", async () => {
    const view = make("app/User.php", USER);
    const other = make("app/Ctrl.php", CONTROLLER);
    await applyWorkspaceEdit(renameEdit("Purchase"), { label: "Rename" });
    other.dispatch({ changes: { from: 0, insert: "// edited\n" } });
    undo(view);
    await new Promise((r) => setTimeout(r));
    expect(view.state.doc.toString()).toBe(USER);
    expect(other.state.doc.toString()).toContain("Purchase");
  });
});

describe("Problems Detected", () => {
  it("lists located conflicts by file for the Find tool window", () => {
    const answer = conflictsAnswer("Rename class Order", [
      { message: "Class Purchase already exists", path: "app/Models/Purchase.php", line: 4 },
      { message: "Purchase.php already exists" },
      { message: "Overlapping edits", path: "app/Models/Purchase.php", line: 9 },
    ]);
    expect(answer.byFile.total).toBe(2);
    expect(answer.byFile.groups[0].files).toEqual([
      expect.objectContaining({ path: "app/Models/Purchase.php", usages: [expect.objectContaining({ line: 4, text: "Class Purchase already exists" }), expect.objectContaining({ line: 9 })] }),
    ]);
  });
});
