import { describe, expect, it } from "vitest";
import { fitZoom, formatImageInfo, formatSize, nextZoom } from "./zoom";

describe("image editor zoom", () => {
  it("steps through zoom levels", () => {
    expect(nextZoom(1, 1)).toBe(1.5);
    expect(nextZoom(1, -1)).toBe(0.75);
    expect(nextZoom(0.8, 1)).toBe(1);
    expect(nextZoom(32, 1)).toBe(32);
    expect(nextZoom(0.05, -1)).toBe(0.05);
  });

  it("fits images into the window", () => {
    expect(fitZoom(2000, 1000, 1000, 800)).toBe(0.5);
    expect(fitZoom(10, 10, 100, 50)).toBe(5);
    expect(fitZoom(0, 10, 100, 50)).toBe(1);
  });

  it("describes images like PHPStorm", () => {
    expect(formatSize(484)).toBe("484 B");
    expect(formatSize(4170)).toBe("4.17 kB");
    expect(formatSize(2_100_000)).toBe("2.1 MB");
    expect(formatImageInfo({ format: "PNG", mime: "image/png", width: 128, height: 64, size: 484, bitDepth: 32 })).toBe(
      "128x64 PNG (32-bit color) 484 B",
    );
    expect(formatImageInfo({ format: "SVG", mime: "image/svg+xml", width: 24, height: 24, size: 1200 })).toBe("24x24 SVG 1.20 kB");
  });
});
