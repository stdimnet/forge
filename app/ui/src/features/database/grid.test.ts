import { describe, expect, it } from "vitest";
import type { TableInfo } from "@/ipc/db";
import { buildChanges, emptyPending, gridColumns, kindOf, parseInput, pendingCount, rangeText, renderCell, typeLabel } from "./grid";

const table: TableInfo = {
  name: "users",
  kind: "table",
  columns: [
    { name: "id", dataType: "bigint unsigned", nullable: false, autoIncrement: true },
    { name: "name", dataType: "varchar(255)", nullable: false, autoIncrement: false },
    { name: "balance", dataType: "decimal(10,2)", nullable: false, autoIncrement: false },
    { name: "avatar", dataType: "blob", nullable: true, autoIncrement: false },
  ],
  primaryKey: ["id"],
  foreignKeys: [],
  indexes: [],
};

const cols = gridColumns(
  [
    { name: "id", typeName: "BIGINT UNSIGNED" },
    { name: "name", typeName: "VARCHAR" },
    { name: "balance", typeName: "DECIMAL" },
    { name: "avatar", typeName: "BLOB" },
  ],
  table,
);

describe("data grid model", () => {
  it("classifies column types", () => {
    expect(kindOf("BIGINT UNSIGNED")).toBe("number");
    expect(kindOf("tinyint(1)")).toBe("bool");
    expect(kindOf("timestamptz")).toBe("date");
    expect(kindOf("bytea")).toBe("binary");
    expect(kindOf("enum('a','b')")).toBe("text");
    expect(cols.map((c) => c.kind)).toEqual(["number", "text", "number", "binary"]);
    expect(cols[0].primary).toBe(true);
    expect(typeLabel(cols[0])).toBe("bigint");
    expect(typeLabel(cols[1])).toBe("varchar(255)");
  });

  it("renders NULL, binary and numbers", () => {
    expect(renderCell(null, "text")).toEqual({ text: "<null>", cls: "null" });
    expect(renderCell({ $hex: "89504e47" }, "binary")).toEqual({ text: "0x89504E47", cls: "binary" });
    expect(renderCell("19.99", "number").cls).toBe("number");
    expect(renderCell("a\nb", "text").text).toBe("a↵b");
  });

  it("parses typed input", () => {
    expect(parseInput("42", "number", 1)).toBe(42);
    expect(parseInput("42.50", "number", "1.00")).toBe("42.50");
    expect(parseInput("yes", "bool", false)).toBe(true);
    expect(parseInput("0xFF", "binary", null)).toEqual({ $hex: "ff" });
    expect(parseInput("x", "text", null)).toBe("x");
  });

  it("builds row changes from pending edits", () => {
    const rows = [
      [1, "Ada", "1.00", null],
      [2, "Alan", "2.00", null],
      [3, "Grace", "3.00", null],
    ];
    const p = emptyPending();
    p.edits[0] = { 1: "Ada King" };
    p.edits[2] = { 2: "9.00" };
    p.deleted.push(2, 1);
    p.inserted.push([undefined, "Linus", "0.00", undefined]);
    expect(pendingCount(p)).toBe(4);
    // Keys carry the whole original row; the backend picks the identifying columns.
    const key = (i: number) => Object.fromEntries(cols.map((c, k) => [c.name, rows[i][k]]));
    expect(buildChanges(cols, rows, p)).toEqual([
      { kind: "update", key: key(0), values: { name: "Ada King" } },
      { kind: "delete", key: key(1), values: {} },
      { kind: "delete", key: key(2), values: {} },
      { kind: "insert", key: {}, values: { name: "Linus", balance: "0.00" } },
    ]);
  });

  it("copies ranges as tab-separated text", () => {
    const rows = [
      [1, "Ada", "1.00", { $hex: "ff" }],
      [2, null, "2.00", null],
    ];
    expect(rangeText(cols, rows, 0, 1, 1, 3, true)).toBe("name\tbalance\tavatar\nAda\t1.00\t0xFF\n\t2.00\t");
  });
});
