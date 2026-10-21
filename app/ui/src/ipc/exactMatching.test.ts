import { afterEach, describe, expect, it } from "vitest";
import { exactMatching, setExactOverride } from "./search";

afterEach(() => setExactOverride(null));

describe("exactMatching", () => {
  it("is fuzzy by default and follows the popup toggle", () => {
    expect(exactMatching()).toBe(false);
    setExactOverride(true);
    expect(exactMatching()).toBe(true);
    setExactOverride(false);
    expect(exactMatching()).toBe(false);
  });
});
