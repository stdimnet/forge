import { describe, expect, it } from "vitest";
import { foundTitle } from "./store";

describe("foundTitle", () => {
  it("pluralizes vulnerabilities and packages separately", () => {
    expect(foundTitle(1, 1)).toBe("Found 1 vulnerability in 1 package");
    expect(foundTitle(3, 2)).toBe("Found 3 vulnerabilities in 2 packages");
    expect(foundTitle(2, 1)).toBe("Found 2 vulnerabilities in 1 package");
  });
});
