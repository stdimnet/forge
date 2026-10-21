import { describe, expect, it } from "vitest";
import { lineOfOffset, offsetOfLine } from "./syncScroll";

// Heading on line 0 (0..40px), paragraph on lines 2-5 (60..160px), code on lines 8-10 (200..260px).
const anchors = [
  { start: 0, end: 1, top: 0, bottom: 40 },
  { start: 2, end: 6, top: 60, bottom: 160 },
  { start: 8, end: 11, top: 200, bottom: 260 },
];

describe("markdown sync scroll", () => {
  it("maps source lines to preview offsets, interpolating inside blocks and gaps", () => {
    expect(offsetOfLine(anchors, 0)).toBe(0);
    expect(offsetOfLine(anchors, 4)).toBe(110);
    expect(offsetOfLine(anchors, 1.5)).toBe(50);
    expect(offsetOfLine(anchors, 20)).toBe(260);
  });

  it("maps preview offsets back to the same lines", () => {
    for (const line of [0, 0.5, 2, 4, 7, 9]) {
      expect(lineOfOffset(anchors, offsetOfLine(anchors, line))).toBeCloseTo(line, 5);
    }
    expect(lineOfOffset([], 100)).toBe(0);
  });
});
