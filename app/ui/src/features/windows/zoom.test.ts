import { describe, expect, it } from "vitest";
import { nextZoom, zoomPercent } from "./zoom";

describe("IDE zoom", () => {
  it("steps through the zoom levels and stops at the ends", () => {
    expect(nextZoom(100, 1)).toBe(110);
    expect(nextZoom(100, -1)).toBe(90);
    expect(nextZoom(95, 1)).toBe(100);
    expect(nextZoom(95, -1)).toBe(90);
    expect(nextZoom(300, 1)).toBe(300);
    expect(nextZoom(50, -1)).toBe(50);
  });

  it("reads the setting leniently", () => {
    expect(zoomPercent("125")).toBe(125);
    expect(zoomPercent(150)).toBe(150);
    expect(zoomPercent("huge")).toBe(100);
    expect(zoomPercent("900")).toBe(100);
    expect(zoomPercent(undefined)).toBe(100);
  });
});
