import { describe, expect, it } from "vitest";
import { formatDuration } from "./duration";

describe("formatDuration", () => {
  it("uses ms below a second and seconds above", () => {
    expect(formatDuration(0.2)).toBe("1 ms");
    expect(formatDuration(850.4)).toBe("850 ms");
    expect(formatDuration(1400)).toBe("1.40 s");
    expect(formatDuration(12345)).toBe("12.3 s");
  });
});
