import { describe, expect, it } from "vitest";
import { cleanSourcePath, displayPath, formatMappings, normalizeSegments, parseMappings, toOpenTarget } from "../src/lib/paths";

const ROOT = "/home/me/shop";
const ctx = { roots: [ROOT] };

describe("cleanSourcePath", () => {
  it.each([
    ["webpack:///./src/App.tsx", "src/App.tsx"],
    ["webpack://my-app/./src/components/Card.vue?vue&type=template", "src/components/Card.vue"],
    ["webpack:///src/App.js", "src/App.js"],
    ["webpack-internal:///./src/App.tsx", "src/App.tsx"],
    ["http://localhost:5173/src/App.tsx?t=1712", "/src/App.tsx"],
    ["https://shop.test/resources/js/app.ts#x", "/resources/js/app.ts"],
    ["http://localhost:5173/@fs/home/me/lib/ui/Button.tsx?v=abc", "/home/me/lib/ui/Button.tsx"],
    ["/@fs/C:/work/shop/src/a.ts", "C:/work/shop/src/a.ts"],
    ["file:///home/me/My%20Shop/src/a.ts", "/home/me/My Shop/src/a.ts"],
    ["file:///C:/work/a.ts", "C:/work/a.ts"],
    ["C:\\work\\shop\\src\\a.ts", "C:/work/shop/src/a.ts"],
    ["./src/../src/App.tsx", "src/App.tsx"],
    ["/home/me/shop/src/App.tsx", "/home/me/shop/src/App.tsx"],
  ])("%s → %s", (raw, expected) => {
    expect(cleanSourcePath(raw)).toBe(expected);
  });

  it("never climbs above the root", () => {
    expect(normalizeSegments("/a/../../etc/passwd")).toBe("/etc/passwd");
    expect(normalizeSegments("../x")).toBe("../x");
  });
});

describe("toOpenTarget", () => {
  it("keeps absolute paths", () => {
    expect(toOpenTarget("/home/me/shop/src/App.tsx", ctx, { line: 12, column: 3 })).toEqual({
      file: "/home/me/shop/src/App.tsx",
      line: 12,
      column: 3,
      symbol: undefined,
    });
    expect(toOpenTarget("/Users/me/other/x.ts", ctx).file).toBe("/Users/me/other/x.ts");
  });

  it("joins relative and dev-server paths to the project", () => {
    expect(toOpenTarget("webpack:///./src/App.tsx", ctx)).toMatchObject({ file: `${ROOT}/src/App.tsx`, project: ROOT });
    expect(toOpenTarget("http://localhost:5173/src/main.ts?t=1", ctx)).toMatchObject({
      file: `${ROOT}/src/main.ts`,
      project: ROOT,
    });
    expect(toOpenTarget("src/components/Card.vue", ctx).file).toBe(`${ROOT}/src/components/Card.vue`);
  });

  it("leaves relative paths for Forge when no project is known", () => {
    expect(toOpenTarget("./src/App.tsx", { roots: [] })).toMatchObject({ file: "src/App.tsx" });
    expect(toOpenTarget("/src/App.tsx", { roots: [] }).file).toBe("/src/App.tsx");
  });

  it("applies path mappings (Docker, remote dev)", () => {
    const docker = { roots: [ROOT], mappings: [{ from: "/var/www/html/", to: ROOT }] };
    expect(toOpenTarget("/var/www/html/app/Models/User.php", docker, { line: 7 })).toMatchObject({
      file: `${ROOT}/app/Models/User.php`,
      line: 7,
    });
    expect(toOpenTarget("/var/www/html-old/x.php", docker).file).toBe("/var/www/html-old/x.php");
  });

  it("passes the component name for declaration lookup", () => {
    expect(toOpenTarget("src/Cart.tsx", ctx, { symbol: "Cart" }).symbol).toBe("Cart");
  });
});

describe("mappings text", () => {
  it("round-trips the popup's textarea format", () => {
    const text = "/var/www/html => /home/me/shop\n\n  bogus line\n/app=>/home/me/api ";
    const mappings = parseMappings(text);
    expect(mappings).toEqual([
      { from: "/var/www/html", to: "/home/me/shop" },
      { from: "/app", to: "/home/me/api" },
    ]);
    expect(formatMappings(mappings)).toBe("/var/www/html => /home/me/shop\n/app => /home/me/api");
  });
});

describe("displayPath", () => {
  it("shortens to the project or the last segments", () => {
    expect(displayPath(`${ROOT}/src/components/Card.tsx`, [ROOT])).toBe("src/components/Card.tsx");
    expect(displayPath("webpack:///./a/b/c/d/Card.tsx", [])).toBe("c/d/Card.tsx");
  });
});
