import { describe, expect, it } from "vitest";
import { isRawLayer, zIndexViolations } from "./zindexCheck.mjs";

describe("zindex check", () => {
  it("allows layer variables and small local values", () => {
    expect(isRawLayer("var(--z-popup)")).toBe(false);
    expect(isRawLayer("calc(var(--z-modal) + 1)")).toBe(false);
    expect(isRawLayer("10")).toBe(false);
    expect(isRawLayer("-1")).toBe(false);
    expect(isRawLayer("1000")).toBe(true);
    expect(isRawLayer("calc(300 + 1)")).toBe(true);
  });

  it("finds raw values in stylesheets, skipping comments", () => {
    const css = `.a {\n  z-index: 5;\n}\n/* z-index: 900; */\n.b {\n  z-index: 345;\n}\n.c { z-index: var(--z-tooltip); }`;
    expect(zIndexViolations(css, true)).toEqual([{ line: 6, value: "345" }]);
  });

  it("finds raw values in style objects and assignments", () => {
    const ts = `const a = { zIndex: "6" };\nconst b = { "z-index": 1000 };\nel.style.zIndex = "450";\n// zIndex: 999\nel.style.zIndex = String(layer.z);`;
    expect(zIndexViolations(ts, false).map((v) => v.line)).toEqual([2, 3]);
  });
});
