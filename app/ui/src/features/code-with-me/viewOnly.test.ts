import { describe, expect, it } from "vitest";
import { decodeOpened, encodeOpened } from "@/ipc/collab";
import { viewOnlyText } from "./binding";

describe("view-only shared files", () => {
  it("names the reason and the size", () => {
    expect(viewOnlyText({ kind: "large", size: 3_400_000 })).toContain("3.4 MB");
    expect(viewOnlyText({ kind: "large", size: 3_400_000 })).toContain("too large");
    expect(viewOnlyText({ kind: "binary", size: 484 })).toContain("484 B");
    expect(viewOnlyText({ kind: "binary", size: 484 })).toContain("Binary");
  });

  it("keeps the view in the raw collab_doc_open reply", () => {
    const opened = { docId: 3, path: "logo.png", update: new Uint8Array([0, 0]), saved: { hash: "0:1" }, readOnly: true, view: { kind: "binary" as const, size: 10 } };
    expect(decodeOpened(encodeOpened(opened)).view).toEqual({ kind: "binary", size: 10 });
    const { view, ...older } = opened;
    void view;
    expect(decodeOpened(encodeOpened(older)).view).toBeUndefined();
  });
});
