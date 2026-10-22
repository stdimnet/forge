import { describe, expect, it } from "vitest";
import type { Breakpoint, Property } from "./api";
import { copyText, phpLiteral } from "./format";
import { typedPrefix, wantsCompletion } from "./ExpressionInput";
import { breakpointLook } from "./editorDebug";
import { httpRequestText } from "./runTypes";
import { builtinServerUrl } from "@/features/run/builtinTypes";

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

describe("Set Value and Copy Value", () => {
  it("starts editing with a PHP literal", () => {
    expect(phpLiteral(prop({ type: "string", value: "it's \\ ok" }))).toBe("'it\\'s \\\\ ok'");
    expect(phpLiteral(prop({ type: "int", value: "42" }))).toBe("42");
    expect(phpLiteral(prop({ type: "bool", value: "0" }))).toBe("false");
    expect(phpLiteral(prop({ type: "null" }))).toBe("null");
    expect(phpLiteral(prop({ type: "array", numChildren: 2 }))).toBe("");
  });

  it("copies strings without quotes", () => {
    expect(copyText(prop({ type: "string", value: "PAID" }))).toBe("PAID");
    expect(copyText(prop({ type: "float", value: "1.5" }))).toBe("1.5");
  });
});

describe("console completion", () => {
  it("asks for completions after $ and ->", () => {
    expect(wantsCompletion("count($or")).toBe(true);
    expect(wantsCompletion("$order->")).toBe(true);
    expect(wantsCompletion("$order->status ")).toBe(false);
    expect(wantsCompletion("strlen(")).toBe(false);
    expect(typedPrefix("$order->sta")).toBe("sta");
    expect(typedPrefix("$")).toBe("");
  });
});

describe("breakpoint looks", () => {
  const bp = (p: Partial<Breakpoint>): Breakpoint => ({ id: 1, kind: "line", file: "a.php", line: 3, enabled: true, ...p });
  it("shows logpoints apart from suspending breakpoints", () => {
    expect(breakpointLook(bp({ suspend: false }), false)).toBe("log");
    expect(breakpointLook(bp({ suspend: false, enabled: false }), false)).toBe("disabled");
    expect(breakpointLook(bp({ suspend: false }), true)).toBe("muted");
    expect(breakpointLook(bp({ condition: "$a" }), false)).toBe("conditional");
    expect(breakpointLook(bp({}), false)).toBe("enabled");
  });
});

describe("web run configurations", () => {
  it("builds the request text of PHP HTTP Request", () => {
    expect(httpRequestText("POST", "http://shop.test/api", "Accept: application/json\nbogus\n", '{"a":1}')).toBe(
      'POST http://shop.test/api\nAccept: application/json\n\n{"a":1}\n',
    );
    expect(httpRequestText("", "http://x/", "", "")).toBe("GET http://x/\n");
  });

  it("opens the built-in server's URL", () => {
    const cfg = (options: Record<string, string>) => ({ name: "s", type: "php-builtin-server", options });
    expect(builtinServerUrl(cfg({}))).toBe("http://localhost:8080/");
    expect(builtinServerUrl(cfg({ host: "0.0.0.0", port: "8000" }))).toBe("http://localhost:8000/");
    expect(builtinServerUrl(cfg({ host: "shop.test", port: "80" }))).toBe("http://shop.test:80/");
  });
});
