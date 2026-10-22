import { describe, expect, it } from "vitest";
import { callWasmSync, forgeWasm } from "@/wasm";
import { fileManagerName } from "./selectIn";

describe("editor navigation through WebAssembly", () => {
  it("walks the edit history back and forward", async () => {
    const w = await forgeWasm();
    const run = (ops: unknown[], list = false) =>
      callWasmSync<{ place: { path: string; line: number } | null; places?: { path: string; line: number }[] }>(w.edit_history, { ops, list });
    run([{ op: "clear" }]);
    const out = run(
      [
        { op: "record", path: "a.php", line: 3, column: 0 },
        { op: "record", path: "a.php", line: 4, column: 2 },
        { op: "record", path: "b.php", line: 9, column: 0 },
        { op: "back", caret: { path: "b.php", line: 9 } },
      ],
      true,
    );
    expect(out.place).toMatchObject({ path: "a.php", line: 4 });
    expect(out.places?.map((p) => `${p.path}:${p.line}`)).toEqual(["b.php:9", "a.php:4"]);
    expect(run([{ op: "forward", caret: { path: "a.php", line: 4 } }]).place).toMatchObject({ path: "b.php" });
  });

  it("finds the next method and change", async () => {
    const w = await forgeWasm();
    const text = "<?php\nclass A {\n  function b() {}\n  function c() {}\n}\n";
    const next = callWasmSync<{ line: number; column: number } | null>(w.editor_method_target, { text, language: "php", line: 2, column: 20, forward: true });
    expect(next).toEqual({ line: 3, column: 11 });
    const ranges = [{ start: 1, end: 2 }, { start: 5, end: 5 }];
    expect(callWasmSync<number | null>(w.editor_change_target, { ranges, line: 1, forward: true })).toBe(1);
    expect(callWasmSync<number | null>(w.editor_change_target, { ranges, line: 1, forward: false })).toBe(1);
  });

  it("names the file manager per OS", () => {
    expect(fileManagerName("windows")).toBe("Explorer");
    expect(fileManagerName("macos")).toBe("Finder");
    expect(fileManagerName("linux")).toBe("File Manager");
  });
});
