import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { ReplaceDone, ReplacePreviewFile, UndoneReplace } from "@/ipc/find";
import { mockFiles } from "./workspaceMock";
import "./replaceMock";

describe("replace mocks", () => {
  it("previews, replaces and undoes in the in-memory project", async () => {
    const path = Object.keys(mockFiles).find((p) => p.endsWith(".php") && /Order/.test(mockFiles[p]))!;
    const before = mockFiles[path];
    const request = { query: "Order", caseSensitive: true, limit: 100000 };

    const preview = await invoke<ReplacePreviewFile[]>("replace_preview", { request, with: "Purchase" });
    expect(preview.length).toBeGreaterThan(0);
    expect(preview[0].lines[0].after).toContain("Purchase");
    expect(mockFiles[path]).toBe(before);

    const done = await invoke<ReplaceDone>("replace_in_files", { request, with: "Purchase" });
    expect(done.replacements).toBeGreaterThan(0);
    expect(mockFiles[path]).not.toContain("Order");

    const undone = await invoke<UndoneReplace>("undo_replace");
    expect(undone.restored).toBe(done.files);
    expect(mockFiles[path]).toBe(before);
  });
});
