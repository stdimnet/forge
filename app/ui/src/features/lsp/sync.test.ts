import { afterEach, describe, expect, it, vi } from "vitest";
import { EditorView } from "@codemirror/view";
import { EditorState } from "@codemirror/state";

const lsp = vi.hoisted(() => ({ didOpen: vi.fn(), didClose: vi.fn(), didChange: vi.fn(), didSave: vi.fn() }));
vi.mock("@/ipc/lsp", async (orig) => ({
  ...(await orig<typeof import("@/ipc/lsp")>()),
  lspApi: {
    didOpen: (...a: unknown[]) => (lsp.didOpen(...a), Promise.resolve()),
    didClose: (...a: unknown[]) => (lsp.didClose(...a), Promise.resolve()),
    didChange: (...a: unknown[]) => (lsp.didChange(...a), Promise.resolve()),
    didSave: (...a: unknown[]) => (lsp.didSave(...a), Promise.resolve()),
  },
}));

const { documentSync, reopenDocuments } = await import("./sync");

const editorOf = (path: string) => new EditorView({ state: EditorState.create({ doc: "<?php\n", extensions: [documentSync(path)] }) });

afterEach(() => vi.clearAllMocks());

describe("document sync", () => {
  it("keeps a file open for the server while another split view still shows it", () => {
    const left = editorOf("app/User.php");
    const right = editorOf("app/User.php");
    expect(lsp.didOpen).toHaveBeenCalledOnce();
    left.destroy();
    expect(lsp.didClose).not.toHaveBeenCalled();
    reopenDocuments();
    expect(lsp.didOpen).toHaveBeenCalledTimes(2);
    right.destroy();
    expect(lsp.didClose).toHaveBeenCalledExactlyOnceWith("app/User.php");
  });
});
