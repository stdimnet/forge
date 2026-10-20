import { beforeEach, describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { DataSourceInfo, QueryPage, SessionInfo, StatementResult, TestResult } from "@/ipc/db";
import { createDemoDatabase } from "./dbDemoData";
import { runStatement } from "./dbEngine";
import { resetDbMock } from "./dbMock";

async function addShop(): Promise<DataSourceInfo> {
  const [ds] = await invoke<DataSourceInfo[]>("db_add_from_env");
  return ds;
}

describe("db engine", () => {
  const db = createDemoDatabase();

  it("selects with where, order and limit", () => {
    const r = runStatement(db, "SELECT id, name FROM users WHERE role = 'admin' ORDER BY id DESC LIMIT 1");
    expect(r.columns.map((c) => c.name)).toEqual(["id", "name"]);
    expect(r.rows).toEqual([[2, "Alan Turing"]]);
  });

  it("types result columns like MySQL", () => {
    const r = runStatement(db, "select * from products limit 1");
    const types = Object.fromEntries(r.columns.map((c) => [c.name, c.typeName]));
    expect(types).toMatchObject({ id: "BIGINT UNSIGNED", price: "DECIMAL", is_active: "BOOLEAN", name: "VARCHAR" });
    expect(typeof r.rows[0][3]).toBe("string");
  });

  it("counts, filters with LIKE/IN/IS NULL and qualifies schemas", () => {
    expect(runStatement(db, "SELECT COUNT(*) FROM shop.orders WHERE status IN ('paid', 'shipped')").rows[0][0]).toBeGreaterThan(0);
    expect(runStatement(db, "SELECT email FROM users WHERE email LIKE 'ada.l%'").rows).toEqual([["ada.lovelace@example.com"]]);
    expect(runStatement(db, "SELECT id FROM users WHERE email_verified_at IS NULL").rows.length).toBe(9);
  });

  it("reports unknown tables and columns", () => {
    expect(() => runStatement(db, "SELECT * FROM nope")).toThrow("doesn't exist");
    expect(() => runStatement(db, "SELECT nope FROM users")).toThrow("Unknown column");
  });

  it("updates, inserts and deletes", () => {
    const d = createDemoDatabase();
    expect(runStatement(d, "UPDATE users SET balance = 0 WHERE id = 3").rowsAffected).toBe(1);
    expect(runStatement(d, "SELECT balance FROM users WHERE id = 3").rows[0][0]).toBe("0.00");
    expect(runStatement(d, "INSERT INTO migrations (migration, batch) VALUES ('x', 4)").rowsAffected).toBe(1);
    expect(runStatement(d, "SELECT id FROM migrations WHERE batch = 4").rows).toEqual([[8]]);
    expect(() => runStatement(d, "INSERT INTO users (name, email, password) VALUES ('A', 'ada.lovelace@example.com', 'x')")).toThrow("Duplicate entry");
    expect(runStatement(d, "DELETE FROM migrations WHERE batch = 4").rowsAffected).toBe(1);
  });
});

describe("db mock", () => {
  beforeEach(() => resetDbMock());

  it("suggests the .env data source until it is added", async () => {
    expect(await invoke<unknown[]>("db_detect_env")).toHaveLength(1);
    const ds = await addShop();
    expect(ds).toMatchObject({ name: "shop@localhost", driver: "mysql", hasPassword: true, comment: "From .env" });
    expect(await invoke<unknown[]>("db_detect_env")).toHaveLength(0);
  });

  it("tests connections", async () => {
    const ds = await addShop();
    const ok = await invoke<TestResult>("db_test_connection", { source: ds });
    expect(ok).toMatchObject({ ok: true, serverVersion: "8.0.36" });
    const bad = await invoke<TestResult>("db_test_connection", { source: ds, password: "wrong" });
    expect(bad.ok).toBe(false);
    expect(bad.error).toContain("Access denied");
  });

  it("runs console scripts and pages results", async () => {
    const ds = await addShop();
    const s = await invoke<SessionInfo>("db_session_open", { id: ds.id });
    const res = await invoke<StatementResult[]>("db_execute", {
      sessionId: s.id,
      sql: "SELECT * FROM orders; UPDATE products SET stock = 1 WHERE id = 1; SELECT * FROM missing; SELECT 1",
      pageSize: 50,
    });
    expect(res).toHaveLength(3);
    expect(res[0].rows).toHaveLength(50);
    expect(res[0].hasMore).toBe(true);
    expect(res[1].rowsAffected).toBe(1);
    expect(res[2].error).toContain("doesn't exist");
    const page = await invoke<QueryPage>("db_fetch", { sessionId: s.id, sql: "SELECT * FROM orders", offset: 100, limit: 50 });
    expect(page.rows).toHaveLength(20);
    expect(page.hasMore).toBe(false);
  });

  it("rolls back manual transactions", async () => {
    const ds = await addShop();
    const s = await invoke<SessionInfo>("db_session_open", { id: ds.id });
    await invoke("db_set_auto_commit", { sessionId: s.id, autoCommit: false });
    await invoke("db_execute", { sessionId: s.id, sql: "DELETE FROM audit_log" });
    const count = async () =>
      (await invoke<StatementResult[]>("db_execute", { sessionId: s.id, sql: "SELECT COUNT(*) FROM audit_log" }))[0].rows[0][0];
    expect(await count()).toBe(0);
    const info = await invoke<SessionInfo>("db_rollback", { sessionId: s.id });
    expect(info.txOpen).toBe(false);
    expect(await count()).toBe(12);
  });

  it("pages table data with filter and sort", async () => {
    const ds = await addShop();
    const page = await invoke<QueryPage>("db_table_data", {
      id: ds.id,
      request: { schema: "shop", table: "products", filter: "price > 100", order: [{ column: "price", desc: true }], limit: 2 },
    });
    expect(page.total).toBe(5);
    expect(page.rows.map((r) => r[2])).toEqual(["Standing Desk Frame", "Noise-Cancelling Headphones"]);
    expect(page.hasMore).toBe(true);
  });

  it("applies row changes and refuses tables without a primary key", async () => {
    const ds = await addShop();
    const res = await invoke<{ statements: string[] }>("db_apply_changes", {
      id: ds.id,
      schema: "shop",
      table: "users",
      changes: [{ kind: "update", key: { id: 1 }, values: { name: "Ada King" } }],
    });
    expect(res.statements).toEqual(["UPDATE `shop`.`users` SET `name` = 'Ada King' WHERE `id` = 1"]);
    await expect(
      invoke("db_apply_changes", {
        id: ds.id,
        schema: "shop",
        table: "audit_log",
        changes: [{ kind: "delete", key: {}, values: {} }],
      }),
    ).rejects.toThrow("no primary key");
  });
});
