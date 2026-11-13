import { describe, expect, it } from "vitest";
import { registerMock } from "@/ipc";
import { onPathMoved } from "./editor";
import { deletePath, renamePath } from "./pathOps";

describe("path operations", () => {
  it("tell per-path state about renames and deletes, only after they succeeded", async () => {
    const moves: [string, string | null][] = [];
    const stop = onPathMoved((from, to) => void moves.push([from, to]));
    registerMock("fs_rename", () => undefined);
    registerMock("fs_delete", ({ path }) => {
      if (path === "locked.php") throw new Error("permission denied");
    });
    await renamePath("app/Foo.php", "app/Bar.php");
    await deletePath("app/Old");
    await expect(deletePath("locked.php")).rejects.toThrow("permission denied");
    stop();
    expect(moves).toEqual([
      ["app/Foo.php", "app/Bar.php"],
      ["app/Old", null],
    ]);
  });
});
