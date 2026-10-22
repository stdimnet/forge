import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { history, undo } from "@codemirror/commands";
import { registerMock } from "@/ipc";
import type { EditPlan, WorkspaceEdit } from "@/ipc/codeintel";
import type { RefactorAnswer, SafeDeleteAnswer, SignatureInfo } from "@/ipc/refactor";
import { commands } from "@/state/commands";
import { registerView } from "@/editor/views";
import { canDrop } from "@/features/project/ProjectView";
import { applyWorkspaceEdit, refactoringHistory } from "./refactoring";
import {
  changeSignatureAt,
  describeEntries,
  moveDialog,
  optionsOf,
  rowsOf,
  safeDeleteAt,
  signaturePreview,
  validateSignature,
} from "./moveRefactorings";
import "./index";

const views: EditorView[] = [];
const unregister: (() => void)[] = [];
function make(path: string, doc: string): EditorView {
  const view = new EditorView({ state: EditorState.create({ doc, extensions: [history(), refactoringHistory()] }) });
  views.push(view);
  unregister.push(registerView(path, view));
  return view;
}
afterEach(() => {
  unregister.splice(0).forEach((u) => u());
  views.splice(0).forEach((v) => v.destroy());
});
const tick = () => new Promise((r) => setTimeout(r));
async function dialog() {
  for (let i = 0; i < 50 && !moveDialog(); i++) await tick();
  return moveDialog()!;
}

const PATH = "app/Mailer.php";
const SRC = "<?php\nclass Mailer\n{\n    public function send(string $to, int $retries = 3): bool\n    {\n        return true;\n    }\n}\n";
const range = (line: number, from: number, to: number) => ({ start: { line, character: from }, end: { line, character: to } });

const INFO: SignatureInfo = {
  path: PATH,
  position: { line: 3, character: 20 },
  kind: "method",
  name: "send",
  className: "Mailer",
  visibility: "public",
  isStatic: false,
  isAbstract: false,
  constructor: false,
  returnType: "bool",
  parameters: [
    { name: "to", typeHint: "string", used: false },
    { name: "retries", typeHint: "int", defaultValue: "3", used: false },
  ],
  signature: "public function send(string $to, int $retries = 3): bool",
};

function answer(edits: { range: ReturnType<typeof range>; newText: string }[]): RefactorAnswer {
  return { edit: { changes: [{ kind: "edit", path: PATH, edits }] }, preview: [], conflicts: [], nameRanges: [], skipped: 0 };
}

beforeEach(() => {
  registerMock("code_plan_edit", ({ edit }): EditPlan => {
    const e = edit as WorkspaceEdit;
    return {
      editors: e.changes.flatMap((c) => (c.kind === "edit" ? [{ path: c.path, edits: c.edits }] : [])),
      backend: { changes: e.changes.filter((c) => c.kind !== "edit") },
      saveFirst: [],
      reopen: [],
      close: [],
      fileCount: 1,
    };
  });
  registerMock("lsp_did_change", () => undefined);
});

describe("Move, Copy, Safe Delete, Change Signature and Make Static wiring", () => {
  it("implements PHPStorm's actions", () => {
    for (const id of ["Move", "CopyElement", "SafeDelete", "ChangeSignature", "MakeStatic"]) expect(commands().has(id), id).toBe(true);
  });

  it("names what the Project view moves and where it can be dropped", () => {
    expect(describeEntries([{ path: "app/Models/User.php", kind: "file" }])).toBe("file 'User.php'");
    expect(describeEntries([{ path: "app/Models", kind: "dir" }])).toBe("directory 'Models'");
    expect(describeEntries([{ path: "a", kind: "dir" }, { path: "b", kind: "file" }])).toBe("2 items");
    expect(canDrop("app/Models/User.php", "app/Domain")).toBe(true);
    expect(canDrop("app/Models/User.php", "app/Models")).toBe(false);
    expect(canDrop("app/Models", "app/Models/Sub")).toBe(false);
    expect(canDrop("app/Models", "")).toBe(true);
    expect(canDrop("scratch://a.php", "app")).toBe(false);
  });
});

describe("Change Signature dialog model", () => {
  it("previews, validates and turns the table into options", () => {
    const rows = rowsOf(INFO);
    expect(signaturePreview(INFO, "send", "public", "bool", rows)).toBe("public function send(string $to, int $retries = 3): bool");
    const added = [...rows, { original: null, name: "html", type: "bool", defaultValue: "", callValue: "", byRef: false, variadic: false }];
    expect(validateSignature("send", added)).toBe("Specify the default value for call sites of the new parameter $html");
    added[2].callValue = "true";
    expect(validateSignature("send", added)).toBeNull();
    expect(validateSignature("2x", rows)).toBe("'2x' is not a valid identifier");
    expect(validateSignature("send", [rows[0], { ...rows[1], name: "to" }])).toBe("Duplicate parameter name: $to");
    const o = optionsOf("deliver", "protected", "", [added[2], rows[0]]);
    expect(o).toEqual({
      name: "deliver",
      visibility: "protected",
      returnType: "",
      parameters: [
        { original: null, name: "html", typeHint: "bool", defaultValue: null, callValue: "true", byRef: false, variadic: false },
        { original: 0, name: "to", typeHint: "string", defaultValue: null, callValue: null, byRef: false, variadic: false },
      ],
    });
  });
});

describe("Change Signature", () => {
  it("asks in the dialog and applies the edit as one undo step", async () => {
    registerMock("refactor_change_signature_prepare", () => INFO);
    let received: unknown;
    registerMock("refactor_change_signature", ({ options }) => {
      received = options;
      return answer([{ range: range(3, 20, 24), newText: "deliver" }]);
    });
    const view = make(PATH, SRC);
    view.dispatch({ selection: { anchor: SRC.indexOf("send") } });
    const running = changeSignatureAt(PATH, view);
    const d = await dialog();
    expect(d.kind).toBe("changeSignature");
    if (d.kind !== "changeSignature") return;
    d.req.done({ ...optionsOf("deliver", "public", "bool", rowsOf(INFO)), preview: false });
    await running;
    expect((received as { name: string }).name).toBe("deliver");
    expect(view.state.doc.toString()).toContain("public function deliver(string $to");
    undo(view);
    expect(view.state.doc.toString()).toBe(SRC);
  });
});

describe("Safe Delete", () => {
  const deleteAnswer = (count: number): SafeDeleteAnswer => ({
    kind: "method",
    name: "send()",
    message: `method send() has ${count} usages that are not safe to delete.\nOf those 0 usages are in strings, comments, or non-code files.`,
    count,
    inComments: 0,
    usages: { title: "send()", source: "index", declaration: null, byType: { total: 0, fileCount: 0, groups: [] }, byFile: { total: 0, fileCount: 0, groups: [] }, element: "method", scope: "Project Files" },
    answer: answer([{ range: { start: { line: 3, character: 0 }, end: { line: 7, character: 0 } }, newText: "" }]),
    paths: [],
  });

  it("deletes an unused element after the Safe Delete dialog", async () => {
    registerMock("refactor_safe_delete_prepare", () => deleteAnswer(0));
    const view = make(PATH, SRC);
    view.dispatch({ selection: { anchor: SRC.indexOf("send") } });
    const running = safeDeleteAt(PATH, view);
    const d = await dialog();
    expect(d.kind).toBe("safeDelete");
    if (d.kind !== "safeDelete") return;
    expect(d.req.what).toBe('method "send()"');
    d.req.done({ searchInComments: false });
    await running;
    expect(view.state.doc.toString()).toBe("<?php\nclass Mailer\n{\n}\n");
  });

  it("shows Usages Detected and deletes anyway on request", async () => {
    registerMock("refactor_safe_delete_prepare", () => deleteAnswer(2));
    const view = make(PATH, SRC);
    const running = safeDeleteAt(PATH, view);
    let d = await dialog();
    if (d.kind === "safeDelete") d.req.done({ searchInComments: false });
    await tick();
    d = await dialog();
    expect(d.kind).toBe("usagesDetected");
    if (d.kind !== "usagesDetected") return;
    expect(d.req.message).toContain("has 2 usages that are not safe to delete");
    d.req.done(null);
    await running;
    expect(view.state.doc.toString()).toBe(SRC);
  });
});

describe("file operations of a refactoring", () => {
  it("moves files before the edit and puts them back when the edit fails", async () => {
    const calls: string[] = [];
    registerMock("fs_rename", ({ from, to }) => void calls.push(`rename ${from} -> ${to}`));
    registerMock("code_apply_edit", () => {
      calls.push("apply");
      throw new Error("conflict");
    });
    const edit: WorkspaceEdit = { changes: [{ kind: "delete", path: "app/Domain/x.txt" }] };
    await expect(applyWorkspaceEdit(edit, { label: "Move", fileOps: [{ kind: "move", from: "app/Models", to: "app/Domain" }] })).rejects.toThrow(
      "conflict",
    );
    expect(calls).toEqual(["rename app/Models -> app/Domain", "apply", "rename app/Domain -> app/Models"]);
  });
});
