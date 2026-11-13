import { describe, expect, it } from "vitest";
import type { FileTypeRow } from "@/ipc/filetypes";
import { addPattern, joinIgnored, newCustomId, ownerOf, removeCustomType, removePattern, splitIgnored } from "./model";

const row = (id: string, patterns: string[], extra: Partial<FileTypeRow> = {}): FileTypeRow => ({
  id,
  name: id.toUpperCase(),
  patterns,
  added: [],
  removed: [],
  custom: false,
  indent: 4,
  ...extra,
});

describe("file type pattern drafts", () => {
  it("adds, reassigns and removes patterns", () => {
    const php = row("php", ["*.php", "*.phtml"]);
    let entries = addPattern([], php, "*.module");
    expect(entries).toEqual([{ fileType: "php", pattern: "*.module" }]);
    // The same pattern for another type replaces the entry (Rust moves it).
    entries = addPattern(entries, row("yaml", ["*.yml"]), "*.MODULE");
    expect(entries).toEqual([{ fileType: "yaml", pattern: "*.MODULE" }]);
    // A bundled pattern is removed with a "remove" entry, an added one by dropping it.
    entries = removePattern([], php, "*.phtml");
    expect(entries).toEqual([{ fileType: "php", pattern: "*.phtml", action: "remove" }]);
    const changed = row("php", ["*.php", "*.module"], { added: ["*.module"], removed: ["*.phtml"] });
    expect(removePattern([{ fileType: "php", pattern: "*.module" }], changed, "*.module")).toEqual([]);
    // Adding a removed bundled pattern again just drops the removal.
    expect(addPattern([{ fileType: "php", pattern: "*.phtml", action: "remove" }], changed, "*.phtml")).toEqual([]);
  });

  it("finds owners, ids and ignored names", () => {
    const rows = [row("php", ["*.php"]), row("smarty", ["*.tpl"])];
    expect(ownerOf(rows, "*.TPL", "php")?.id).toBe("smarty");
    expect(ownerOf(rows, "*.php", "php")).toBeUndefined();
    expect(newCustomId(["php", "custom-1"])).toBe("custom-3");
    expect(newCustomId(["custom-2", "custom-3"])).toBe("custom-4");
    const removed = removeCustomType(
      [{ id: "custom-1", name: "A" }, { id: "custom-2", name: "B" }],
      [{ fileType: "custom-1", pattern: "*.a" }, { fileType: "php", pattern: "*.inc" }],
      "custom-1",
    );
    expect(removed.custom.map((c) => c.name)).toEqual(["B"]);
    expect(removed.entries).toEqual([{ fileType: "php", pattern: "*.inc" }]);
    expect(splitIgnored(" .git; *.pyc ;;")).toEqual([".git", "*.pyc"]);
    expect(joinIgnored([".git", "*.pyc"])).toBe(".git;*.pyc");
  });
});
