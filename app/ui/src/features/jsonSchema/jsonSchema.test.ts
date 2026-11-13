import { describe, expect, it } from "vitest";
import type { SchemaMapping } from "@/ipc/jsonSchema";
import { invoke } from "@/ipc";
import "@/mock/jsonSchemaMock";
import { assignSchema, entryKind, fileLines, newMappingName } from "./mappings";

const m = (name: string, schema: string, files: string): SchemaMapping => ({ name, schema, version: "", files });

describe("assignSchema", () => {
  it("moves a file to the chosen schema's mapping", () => {
    const before = [m("Mine", "schemas/a.json", "a.json\nb.json"), m("Only", "schemas/o.json", "c.json")];
    const after = assignSchema(before, "c.json", { name: "Mine", schema: "schemas/a.json" });
    expect(after).toEqual([m("Mine", "schemas/a.json", "a.json\nb.json\nc.json")]);
  });

  it("creates a mapping for a built-in schema or no schema", () => {
    const after = assignSchema([m("Mine", "s.json", "a.json")], "a.json", { name: "No JSON schema", schema: "" });
    expect(after).toEqual([m("No JSON schema", "", "a.json")]);
    const built = assignSchema([], "x/package.json", { name: "package.json", schema: "https://json.schemastore.org/package.json" });
    expect(built[0].files).toBe("x/package.json");
  });

  it("keeps mappings without files", () => {
    expect(assignSchema([m("Empty", "s.json", "")], "a.json", { name: "S", schema: "t.json" })).toHaveLength(2);
  });
});

describe("mapping entries", () => {
  it("tells files, directories and patterns apart", () => {
    expect(entryKind("config/app.json")).toBe("file");
    expect(entryKind("config/")).toBe("directory");
    expect(entryKind("*.app.json")).toBe("pattern");
    expect(fileLines(" a \n\nb/ ")).toEqual(["a", "b/"]);
    expect(newMappingName([m("New Schema", "", "")])).toBe("New Schema (2)");
  });
});

describe("browser-mode JSON Schema", () => {
  it("validates composer.json with the bundled schema", async () => {
    const answer = await invoke<{ schema: { name: string }; problems: { message: string }[] }>("json_schema_validate", {
      path: "composer.json",
      language: "json",
      content: '{"name": "acme/shop", "minimum-stability": "nightly"}',
    });
    expect(answer.schema.name).toBe("composer.json");
    expect(answer.problems.map((p) => p.message)).toEqual([
      'Value should be one of: "dev", "alpha", "beta", "rc", "RC", "stable"',
    ]);
  });

  it("completes GitHub workflow keys", async () => {
    const content = "on: push\njobs:\n  test:\n    ru\n";
    const c = await invoke<{ items: { label: string }[] } | null>("json_schema_complete", {
      path: ".github/workflows/ci.yml",
      language: "yaml",
      content,
      offset: content.length - 1,
    });
    expect(c?.items.map((i) => i.label)).toContain("runs-on");
  });

  it("reads bundled schemas for Go to Declaration", async () => {
    const targets = await invoke<{ path: string }[]>("json_schema_goto", {
      path: "package.json",
      language: "json",
      content: '{"scripts": {}}',
      offset: 3,
    });
    expect(targets[0].path).toBe("json-schema://bundled/package.json");
    const file = await invoke<{ text: string }>("json_schema_read", { location: targets[0].path });
    expect(file.text).toContain('"scripts"');
  });
});
