import { describe, expect, it } from "vitest";
import { severityCount } from "./severity";

describe("severityCount", () => {
  it("uses one message per severity with a plural", () => {
    expect(severityCount("error", 1)).toBe("1 error");
    expect(severityCount("error", 3)).toBe("3 errors");
    expect(severityCount("weak", 1)).toBe("1 weak warning");
    expect(severityCount("weak", 2)).toBe("2 weak warnings");
    expect(severityCount("grammar", 2)).toBe("2 grammar errors");
    expect(severityCount("typo", 1)).toBe("1 typo");
    expect(severityCount("problem", 1)).toBe("1 problem");
    expect(severityCount("problem", 12)).toBe("12 problems");
  });
});
