import { beforeEach, describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import "./workspaceMock";
import "./dataeditMock";
import type { DataSourceInfo, QueryPage } from "@/ipc/db";
import { dataEditApi } from "@/ipc/dataedit";
import { resetDbMock } from "./dbMock";

async function addShop(): Promise<DataSourceInfo> {
  const [ds] = await invoke<DataSourceInfo[]>("db_add_from_env");
  return ds;
}

describe("data editor mock", () => {
  beforeEach(() => resetDbMock());

  it("sorts and filters tables with the backend's SQL", async () => {
    const ds = await addShop();
    const page = await invoke<QueryPage>("db_table_data", {
      id: ds.id,
      request: {
        schema: "shop",
        table: "orders",
        filter: "total > 100",
        columnFilters: [{ column: "status", op: "eq", value: "paid" }],
        order: [{ column: "total", desc: true }],
      },
    });
    expect(page.sql).toBe("SELECT * FROM `shop`.`orders` WHERE (total > 100) AND `status` = 'paid' ORDER BY `total` DESC");
    expect(page.rows.length).toBeGreaterThan(0);
    const totals = page.rows.map((r) => Number(r[4]));
    expect(totals).toEqual([...totals].sort((a, b) => b - a));
    await expect(
      invoke("db_table_data", { id: ds.id, request: { schema: "shop", table: "orders", filter: "1=1; DELETE FROM orders" } }),
    ).rejects.toThrow("one condition");
  });

  it("previews DML by key and asks before production writes", async () => {
    const ds = await addShop();
    const users = await invoke<QueryPage>("db_table_data", { id: ds.id, request: { schema: "shop", table: "users", limit: 2 } });
    const row = Object.fromEntries(users.columns.map((c, i) => [c.name, users.rows[0][i]]));
    const p = await dataEditApi.previewChanges(ds.id, "shop", "users", [{ kind: "update", key: row, values: { role: { $default: true } } }]);
    expect(p.statements).toEqual(["UPDATE `shop`.`users` SET `role` = DEFAULT WHERE `id` = 1"]);
    expect(p.key?.source).toBe("primary key");
    await expect(dataEditApi.previewChanges(ds.id, "shop", "audit_log", [{ kind: "delete", key: { actor: "x" }, values: {} }])).rejects.toThrow(
      "no primary key",
    );
    const res = await dbApiApply(ds.id, [{ kind: "update", key: row, values: { role: { $default: true } } }]);
    expect(res.rowsAffected).toBe(1);
  });

  it("navigates foreign keys and computes aggregates, diffs and charts", async () => {
    const fk = { name: "orders_user_id_foreign", columns: ["user_id"], refSchema: "shop", refTable: "users", refColumns: ["id"] };
    const nav = await dataEditApi.navigate({ dialect: "mysql", schema: "shop", table: "orders", fk, columns: ["id", "user_id"], row: [3, 7], direction: "referenced" });
    expect(nav).toMatchObject({ table: "users", filter: "`id` = 7" });
    const agg = await dataEditApi.aggregate(["total"], [["10.50"], ["4.50"], [null]]);
    expect(agg.total).toMatchObject({ count: 3, nulls: 1, sum: "15.00", avg: "7.5" });
    const diff = await dataEditApi.compare({ columns: ["id", "v"], rows: [[1, "a"], [2, "b"]] }, { columns: ["id", "v"], rows: [[2, "c"], [3, "d"]] }, ["id"]);
    expect([diff.removed, diff.changed, diff.added]).toEqual([1, 1, 1]);
    const chart = await dataEditApi.chart(["k", "n"], [["a", 1], ["a", 2], ["b", 5]], { kind: "bar", x: "k", y: ["n"], agg: "sum" });
    expect(chart.series[0].values).toEqual([3, 5]);
    expect(await dataEditApi.view(["n"], [[3], [1], [2]], { order: [{ column: "n", desc: false }] })).toEqual([1, 2, 0]);
  });

  it("formats and validates values", async () => {
    expect((await dataEditApi.format("json", '{"a":[1,2]}')).text).toBe('{\n  "a": [\n    1,\n    2\n  ]\n}');
    const bad = await dataEditApi.format("json", '{"a":}');
    expect(bad.text).toBeNull();
    expect(bad.error?.line).toBe(1);
    expect(await dataEditApi.editor("enum('a','b')", "a")).toEqual({ kind: "enum", values: ["a", "b"], multiple: false });
    expect((await dataEditApi.binary({ $hex: "89504e470d0a1a0a00" }))?.mime).toBe("image/png");
    expect(await dataEditApi.temporal({ kind: "dateTime" }, "2025-1-6 9:05")).toBe("2025-01-06 09:05:00");
  });
});

async function dbApiApply(id: string, changes: unknown[]) {
  return invoke<{ rowsAffected: number }>("db_apply_changes", { id, schema: "shop", table: "users", changes });
}
