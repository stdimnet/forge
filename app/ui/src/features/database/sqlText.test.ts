import { describe, expect, it } from "vitest";
import { cellText, changeSql, exportRows, leadingKeyword, splitStatements, sqlLiteral, statementAt } from "./sqlText";

describe("sql text", () => {
  it("splits like the backend", () => {
    const sql = "select 1; select ';' as a -- ; no\n; /* ; */ select \"a;b\";\n\n-- only comment\n";
    expect(splitStatements(sql).map((s) => s.text)).toEqual(["select 1", "select ';' as a -- ; no", '/* ; */ select "a;b"']);
    expect(splitStatements("select $$a;b$$; select 2", "postgres").map((s) => s.text)).toEqual(["select $$a;b$$", "select 2"]);
    expect(splitStatements("select 'C:\\'; select 2", "postgres")).toHaveLength(2);
  });

  it("finds the statement at the caret or the selection", () => {
    const sql = "SELECT 1;\n\nSELECT *\nFROM users;\n";
    expect(statementAt(sql, 15, 15)?.text).toBe("SELECT *\nFROM users");
    expect(statementAt(sql, 3, 3)?.text).toBe("SELECT 1");
    // After the last statement: the one before the caret.
    expect(statementAt(sql, sql.length, sql.length)?.text).toBe("SELECT *\nFROM users");
    expect(statementAt(sql, 0, 6)?.text).toBe("SELECT");
  });

  it("renders literals and cell text", () => {
    expect(sqlLiteral("mysql", "it's")).toBe("'it''s'");
    expect(sqlLiteral("sqlite", true)).toBe("1");
    expect(sqlLiteral("postgres", { $hex: "ff" })).toBe("'\\xff'::bytea");
    expect(cellText({ $hex: "0aff" })).toBe("0x0AFF");
    expect(cellText(null)).toBe("");
  });

  it("builds change statements in the backend's column order", () => {
    expect(changeSql("mysql", "shop", "users", ["id"], { kind: "update", key: { id: 7 }, values: { name: "A", email: null } })).toBe(
      "UPDATE `shop`.`users` SET `email` = NULL, `name` = 'A' WHERE `id` = 7",
    );
    expect(changeSql("postgres", "public", "t", ["id"], { kind: "insert", key: {}, values: {} })).toBe('INSERT INTO "public"."t" DEFAULT VALUES');
    expect(() => changeSql("mysql", "s", "log", [], { kind: "delete", key: {}, values: {} })).toThrow("no primary key");
  });

  it("exports like the backend", () => {
    const cols = ["id", "name", "avatar"];
    const rows = [
      [1, 'Ada, "the" first', null],
      ["9007199254740993", "Bob", { $hex: "ff00" }],
    ];
    expect(exportRows("csv", "mysql", undefined, cols, rows)).toBe('id,name,avatar\n1,"Ada, ""the"" first",\n9007199254740993,Bob,0xFF00\n');
    expect(JSON.parse(exportRows("json", "mysql", undefined, cols, rows))[1].avatar).toBe("0xFF00");
    expect(exportRows("sql", "postgres", "users", cols, rows.slice(0, 1))).toBe(
      'INSERT INTO "users" ("id", "name", "avatar") VALUES (1, \'Ada, "the" first\', NULL);\n',
    );
  });

  it("reads the leading keyword", () => {
    expect(leadingKeyword("-- x\n (select 1)")).toBe("SELECT");
    expect(leadingKeyword("/* a */ update t")).toBe("UPDATE");
  });
});
