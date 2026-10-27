import { beforeEach, describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import "./workspaceMock";
import type { SchemaInfo, SessionInfo } from "@/ipc/db";
import type { CompareResult, Diagram, ExplainResult } from "@/ipc/dbSchema";
import { addSchemaToolSources, resetDbMock } from "./dbMock";
import "./dbSchemaMock";

describe("schema tools mock", () => {
  beforeEach(() => {
    resetDbMock();
    addSchemaToolSources();
  });

  it("lays out the ER diagram of the demo schema", async () => {
    const schema = await invoke<SchemaInfo>("db_introspect", { id: "shop-pg", schema: "public" });
    const d = await invoke<Diagram>("db_diagram", { request: { schema, tables: ["orders"], neighbours: true } });
    expect(d.nodes.map((n) => n.id).sort()).toEqual(["order_items", "orders", "users"]);
    const users = d.nodes.find((n) => n.id === "users")!;
    const orders = d.nodes.find((n) => n.id === "orders")!;
    expect(users.y).toBeLessThan(orders.y);
    expect(d.edges.find((e) => e.from === "orders")?.to).toBe("users");
  });

  it("compares the shop with its staging copy", async () => {
    const source = await invoke<SchemaInfo>("db_introspect", { id: "shop-pg", schema: "public" });
    const target = await invoke<SchemaInfo>("db_introspect", { id: "shop-staging", schema: "public" });
    const r = await invoke<CompareResult>("db_schema_compare", {
      request: { dialect: "postgres", source: { schema: source }, target: { schema: target }, framework: "laravel", now: 0 },
    });
    expect(r.diff.tables.find((t) => t.name === "legacy_carts")?.kind).toBe("removed");
    expect(r.diff.tables.find((t) => t.name === "audit_log")?.kind).toBe("added");
    expect(r.migration.sql).toContain('ALTER TABLE "public"."orders" DROP COLUMN "coupon_code"');
    expect(r.file?.path).toBe("database/migrations/1970_01_01_000000_update_public_schema.php");
  });

  it("explains a query in a console session", async () => {
    const s = await invoke<SessionInfo>("db_session_open", { id: "shop-pg" });
    const sql = "SELECT * FROM orders o JOIN users u ON u.id = o.user_id ORDER BY o.total";
    const r = await invoke<ExplainResult>("db_explain", { sessionId: s.id, sql, raw: false });
    expect(r.sql).toBe(`EXPLAIN (FORMAT JSON) ${sql}`);
    expect(r.plan?.root.operation).toBe("Sort");
    expect(r.plan?.flame.length).toBeGreaterThan(3);
    const raw = await invoke<ExplainResult>("db_explain", { sessionId: s.id, sql, raw: true });
    expect(raw.plan).toBeUndefined();
    expect(raw.columns[0].name).toBe("QUERY PLAN");
    await expect(invoke("db_explain", { sessionId: s.id, sql: "SELECT * FROM nope", raw: false })).rejects.toThrow("doesn't exist");
  });
});
