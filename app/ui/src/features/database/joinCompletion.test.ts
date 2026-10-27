import { describe, expect, it } from "vitest";
import type { SchemaInfo } from "@/ipc/db";
import { aliasFor, joinSuggestions, referencedTables } from "./joinCompletion";

const schema: SchemaInfo = {
  name: "shop",
  routines: [],
  tables: [
    { name: "users", kind: "table", columns: [{ name: "id", dataType: "int", nullable: false, autoIncrement: true }], primaryKey: ["id"], foreignKeys: [], indexes: [] },
    {
      name: "orders",
      kind: "table",
      columns: [],
      primaryKey: ["id"],
      foreignKeys: [{ name: "orders_user_id_foreign", columns: ["user_id"], refSchema: "shop", refTable: "users", refColumns: ["id"] }],
      indexes: [],
    },
    {
      name: "order_items",
      kind: "table",
      columns: [],
      primaryKey: ["id"],
      foreignKeys: [{ name: "vfk_order_items_order_orders", columns: ["order"], refSchema: "shop", refTable: "orders", refColumns: ["id"], virtual: true }],
      indexes: [],
    },
  ],
};

describe("join completion", () => {
  it("finds referenced tables and aliases", () => {
    const refs = referencedTables([schema], "SELECT * FROM shop.orders AS o JOIN users WHERE");
    expect(refs.map((r) => [r.table.name, r.alias])).toEqual([
      ["orders", "o"],
      ["users", "users"],
    ]);
    expect(aliasFor("order_items", new Set(["oi"]))).toBe("oi1");
  });

  it("offers both directions of real and virtual foreign keys", () => {
    const s = joinSuggestions([schema], "SELECT * FROM orders o JOIN ");
    expect(s.map((x) => x.apply)).toEqual(["users u ON u.id = o.user_id", "order_items oi ON oi.order = o.id"]);
    expect(s[1]!.virtual).toBe(true);
    expect(s[1]!.detail).toContain("virtual");
    expect(joinSuggestions([schema], "SELECT 1 JOIN ")).toEqual([]);
  });
});
