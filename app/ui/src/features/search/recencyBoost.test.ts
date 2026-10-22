import { describe, expect, it } from "vitest";
import { recencyBoost } from "./recencyBoost";

const recent = ["a/One.php", "a/Two.php", "b/Three.php"];

describe("recencyBoost", () => {
  it("rewards recent files, the most recent most", () => {
    expect(recencyBoost("a/One.php", recent)).toBeGreaterThan(recencyBoost("a/Two.php", recent));
    expect(recencyBoost("a/Two.php", recent)).toBeGreaterThan(recencyBoost("b/Three.php", recent));
    expect(recencyBoost("c/Other.php", recent)).toBe(0);
  });

  it("rewards files next to the active one but not the active file itself", () => {
    expect(recencyBoost("c/Sibling.php", [], "c/Active.php")).toBeGreaterThan(0);
    expect(recencyBoost("c/Active.php", [], "c/Active.php")).toBe(0);
    expect(recencyBoost("d/Far.php", [], "c/Active.php")).toBe(0);
  });

  it("never goes below zero for an old recent file", () => {
    const long = Array.from({ length: 40 }, (_, i) => `f${i}.php`);
    expect(recencyBoost("f39.php", long)).toBe(0);
  });
});
