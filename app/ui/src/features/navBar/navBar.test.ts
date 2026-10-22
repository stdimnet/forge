import { describe, expect, it } from "vitest";
import { Text } from "@codemirror/state";
import { fileStructureView, navBarSegments, scopesAt, structureLoaded, structureOf } from "@/features/codeStructure/model";
import type { StructureNode } from "@/features/php/api";
import { relatedTarget } from "@/features/related";
import { invoke } from "@/ipc";
import "@/mock";
import type { RelatedItem } from "@/ipc/codeintel";

await structureLoaded;

const node = (name: string, kind: StructureNode["kind"], line: number, children?: StructureNode[]): StructureNode => ({
  name,
  kind,
  line,
  detail: "",
  modifiers: {},
  offset: 0,
  children,
});

describe("File Structure popup rows", () => {
  const nodes = [node("Cart", "class", 1, [node("items", "property", 2), node("total", "method", 3), node("clear", "method", 6)])];

  it("filters with fuzzy matching and keeps parents", () => {
    const v = fileStructureView({ nodes, query: "tot", sortAlpha: false, showFields: true, showInherited: false, showLambdas: true });
    expect(v.rows.map((r) => r.node.name)).toEqual(["Cart", "total"]);
    expect(v.rows[1].positions).toEqual([0, 1, 2]);
    expect(v.selected).toBe(1);
  });

  it("selects the node at the caret and hides fields", () => {
    const v = fileStructureView({
      nodes,
      query: "",
      sortAlpha: true,
      showFields: false,
      showInherited: false,
      showLambdas: true,
      caretLine: 7,
      enclosing: [{ line: 1, openLine: 1 }, { line: 6, openLine: 6 }],
    });
    expect(v.rows.map((r) => r.node.name)).toEqual(["Cart", "clear", "total"]);
    expect(v.rows[v.selected!].node.name).toBe("clear");
    expect(v.hasFields).toBe(true);
  });
});

describe("Navigation Bar segments", () => {
  it("lists the path and the members at the caret", () => {
    const doc = Text.of(["<?php", "class Cart {", "    public function total() {", "        return 1;", "    }", "}"]);
    const scopes = scopesAt(structureOf(doc, "php"), 3, 8);
    const segs = navBarSegments({ project: "shop", path: "app/Cart.php", scopes, showMembers: true });
    expect(segs.map((s) => s.label)).toEqual(["shop", "app", "Cart.php", "Cart", "total()"]);
    expect(segs.map((s) => s.kind)).toEqual(["project", "directory", "file", "member", "member"]);
  });
});

describe("Related Symbol", () => {
  it("finds a model's test and migration in the demo project", async () => {
    const items = await invoke<RelatedItem[]>("code_related", { path: "app/Models/Order.php" });
    expect(items.map((i) => [i.group, i.path])).toEqual([
      ["Tests", "tests/Feature/OrderTest.php"],
      ["Migrations", "database/migrations/2026_01_01_000000_create_orders_table.php"],
    ]);
    expect(relatedTarget(items[0])).toMatchObject({ name: "OrderTest.php", container: "tests/Feature", wholeFile: true, group: "Tests" });
  });

  it("finds a controller's views and routes", async () => {
    const items = await invoke<RelatedItem[]>("code_related", { path: "app/Http/Controllers/ProductController.php" });
    expect(items.filter((i) => i.kind === "view").map((i) => i.name)).toEqual(["products.index", "products.show"]);
    expect(items.some((i) => i.kind === "route" && i.path === "routes/web.php")).toBe(true);
  });
});
