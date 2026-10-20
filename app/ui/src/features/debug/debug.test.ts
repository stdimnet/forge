import { describe, expect, it } from "vitest";
import type { Property } from "./api";
import { expressionAt, formatValue, frameLabel, inlineValues, scopeStart } from "./format";
import { serverFor, suggestMapping, uriToPath } from "./mapping";
import { breakpointLook } from "./editorDebug";

const prop = (p: Partial<Property>): Property => ({
  name: "$x",
  fullname: "$x",
  type: "int",
  hasChildren: false,
  numChildren: 0,
  page: 0,
  pageSize: 0,
  children: [],
  ...p,
});

describe("value presentation", () => {
  it("formats like PHPStorm", () => {
    expect(formatValue(prop({ type: "int", value: "42" }))).toBe("{int} 42");
    expect(formatValue(prop({ type: "string", value: "a\nb" }))).toBe('"a\\nb"');
    expect(formatValue(prop({ type: "bool", value: "1" }))).toBe("true");
    expect(formatValue(prop({ type: "null" }))).toBe("null");
    expect(formatValue(prop({ type: "object", classname: "App\\Models\\Order", numChildren: 3, hasChildren: true }))).toBe("{App\\Models\\Order} [3]");
    expect(formatValue(prop({ type: "array", numChildren: 2 }))).toBe("{array} [2]");
  });

  it("labels frames", () => {
    expect(frameLabel({ level: 0, file: "app/Http/Controllers/OrderController.php", fileUri: "", line: 24, where: "App\\Http\\Controllers\\OrderController->show" })).toEqual({
      location: "OrderController.php:24",
      where: "App\\Http\\Controllers\\OrderController->show()",
    });
    expect(frameLabel({ level: 1, file: null, fileUri: "file:///srv/index.php", line: 3, where: "{main}" }).location).toBe("index.php:3");
  });
});

describe("inline values", () => {
  const src = [
    "<?php",
    "class A {",
    "    public function show(Order $order)",
    "    {",
    "        $total = $order->total; // $ignored",
    "        $n = 1;",
    "        return $total;",
    "    }",
    "}",
  ];
  const vars = [
    prop({ name: "$order", type: "object", classname: "App\\Models\\Order", numChildren: 30, hasChildren: true }),
    prop({ name: "$total", type: "float", value: "149.9" }),
    prop({ name: "$n", type: "uninitialized" }),
  ];

  it("finds the function scope", () => {
    expect(scopeStart(src, 6)).toBe(3);
  });

  it("annotates lines from the function start to the execution line", () => {
    const m = inlineValues(src, 6, vars);
    expect(m.get(3)).toBe("$order: {Order} [30]");
    expect(m.get(5)).toBe("$total: 149.9   $order: {Order} [30]");
    expect(m.has(6)).toBe(false); // uninitialized
    expect(m.has(7)).toBe(false); // after the execution point
  });

  it("picks the expression under the caret", () => {
    expect(expressionAt("        $total = $order->lines->count();", 20)).toBe("$order->lines->count");
    expect(expressionAt("foo(  )", 5)).toBe("");
  });
});

describe("path mappings", () => {
  it("derives a directory mapping from a matched file", () => {
    expect(suggestMapping("/var/www/html/public/index.php", "public/index.php")).toEqual({ remote: "/var/www/html", local: "" });
    expect(suggestMapping("/srv/app/web/index.php", "public/index.php")).toEqual({ remote: "/srv/app/web", local: "public" });
    expect(suggestMapping("/srv/x.php", "y.php")).toEqual({ remote: "/srv/x.php", local: "y.php" });
    expect(uriToPath("file:///var/www/my%20app/a.php")).toBe("/var/www/my app/a.php");
    expect(uriToPath("file:///C:/www/a.php")).toBe("C:/www/a.php");
  });

  it("creates a server for an incoming connection", () => {
    const s = serverFor(
      { serverName: "shop.test:8080", host: "shop.test", port: 8080 } as never,
      { remote: "/var/www/html", local: "" },
      [{ id: "shop-test" } as never],
    );
    expect(s).toMatchObject({ id: "shop-test-2", name: "shop.test", host: "shop.test", port: 8080, usePathMappings: true });
  });
});

describe("breakpoint look", () => {
  it("reflects enabled, condition and mute", () => {
    const bp = { id: 1, kind: "line" as const, file: "a.php", line: 1, enabled: true };
    expect(breakpointLook(bp, false)).toBe("enabled");
    expect(breakpointLook({ ...bp, condition: "$a" }, false)).toBe("conditional");
    expect(breakpointLook(bp, true)).toBe("muted");
    expect(breakpointLook({ ...bp, enabled: false }, true)).toBe("disabled");
  });
});
