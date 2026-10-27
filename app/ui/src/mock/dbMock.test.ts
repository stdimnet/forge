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

describe("db mock: more engines, guard and sessions", () => {
  beforeEach(() => resetDbMock());

  const demo = async () => {
    const { addDemoSources } = await import("./dbMock");
    addDemoSources();
  };

  it("browses Redis keys and values and runs commands", async () => {
    await demo();
    const keys = await invoke<{ keys: { key: string; type: string; ttlMs: number }[] }>("db_redis_keys", { id: "cache", schema: "db0", pattern: "user:*" });
    expect(keys.keys.map((k) => [k.key, k.type])).toEqual([
      ["user:1", "hash"],
      ["user:1:roles", "set"],
      ["user:2", "hash"],
      ["user:7", "hash"],
    ]);
    const v = await invoke<{ type: string; value: unknown }>("db_redis_value", { id: "cache", schema: "db0", key: "leaderboard:weekly" });
    expect(v.type).toBe("zset");
    expect((v.value as { member: string }[])[0].member).toBe("barbara.liskov");
    const s = await invoke<SessionInfo>("db_session_open", { id: "cache" });
    expect(s.transactions).toBe(false);
    const res = await invoke<StatementResult[]>("db_execute", { sessionId: s.id, sql: "SET motd hello\nGET motd\nHGETALL user:2" });
    expect(res.map((r) => r.error)).toEqual([undefined, undefined, undefined]);
    expect(res[1].rows).toEqual([["hello"]]);
    expect(res[2].columns.map((c) => c.name)).toEqual(["field", "value"]);
  });

  it("finds and aggregates MongoDB documents", async () => {
    await demo();
    const schema = await invoke<{ tables: { name: string; kind: string }[] }>("db_introspect", { id: "catalog", schema: "shop" });
    expect(schema.tables.map((t) => [t.name, t.kind])).toEqual([
      ["orders", "collection"],
      ["products", "collection"],
      ["users", "collection"],
    ]);
    const s = await invoke<SessionInfo>("db_session_open", { id: "catalog" });
    const [r] = await invoke<StatementResult[]>("db_execute", { sessionId: s.id, sql: "db.users.find({ age: { $gt: 40 } }).sort({ age: -1 }).limit(2)" });
    expect(r.error).toBeUndefined();
    expect(r.columns[0].name).toBe("_id");
    expect(r.documents).toHaveLength(2);
    expect(String(r.rows[0][0])).toMatch(/^ObjectId\("/);
    const [g] = await invoke<StatementResult[]>("db_execute", { sessionId: s.id, sql: "db.orders.aggregate([{ $group: { _id: '$status', n: { $sum: 1 } } }])" });
    expect(g.rows.length).toBeGreaterThan(1);
  });

  it("refuses writes on read-only sources and confirms unfiltered changes on production", async () => {
    await demo();
    const g = await invoke<{ confirm?: string }>("db_guard", { id: "sales-prod", sql: "DELETE FROM orders" });
    expect(g.confirm).toContain("production");
    const s = await invoke<SessionInfo>("db_session_open", { id: "sales-prod" });
    await expect(invoke("db_execute", { sessionId: s.id, sql: "DELETE FROM orders" })).rejects.toThrow("production");
    const ok = await invoke<StatementResult[]>("db_execute", { sessionId: s.id, sql: "DELETE FROM orders", confirmed: true });
    expect(ok[0].rowsAffected).toBeGreaterThan(0);

    const shop = await addShop();
    await invoke("db_data_source_save", { source: { ...shop, options: { ...shop.options, readOnly: true } } });
    const ro = await invoke<SessionInfo>("db_session_open", { id: shop.id });
    await expect(invoke("db_execute", { sessionId: ro.id, sql: "UPDATE users SET name = 'x' WHERE id = 1" })).rejects.toThrow("read-only");
  });

  it("shares a session between consoles and lists sessions", async () => {
    const shop = await addShop();
    const a = await invoke<SessionInfo>("db_session_open", { id: shop.id, shared: true });
    const b = await invoke<SessionInfo>("db_session_open", { id: shop.id, shared: true });
    expect(b.id).toBe(a.id);
    expect(b.consoles).toBe(2);
    await invoke("db_set_auto_commit", { sessionId: a.id, autoCommit: false });
    await invoke("db_execute", { sessionId: a.id, sql: "UPDATE users SET balance = 1 WHERE id = 1" });
    const [listed] = await invoke<SessionInfo[]>("db_sessions_list");
    expect(listed.txOpen).toBe(true);
    expect(listed.txStatements).toBe(1);
    await invoke("db_session_close", { sessionId: a.id });
    expect(await invoke<SessionInfo[]>("db_sessions_list")).toHaveLength(1);
    await invoke("db_session_terminate", { sessionId: a.id });
    expect(await invoke<SessionInfo[]>("db_sessions_list")).toHaveLength(0);
  });

  it("asks to trust an unknown SSH host key", async () => {
    await demo();
    const [prod] = (await invoke<DataSourceInfo[]>("db_data_sources_list")).filter((d) => d.id === "sales-prod");
    expect(prod.via).toBe("deploy@bastion.example.com");
    const first = await invoke<TestResult>("db_test_connection", { source: prod });
    expect(first.hostKey?.fingerprint).toMatch(/^SHA256:/);
    const second = await invoke<TestResult>("db_test_connection", { source: prod, acceptHostKey: first.hostKey!.fingerprint });
    expect(second.ok).toBe(true);
  });
});
