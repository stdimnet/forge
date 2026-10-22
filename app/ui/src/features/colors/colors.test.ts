import { describe, expect, it } from "vitest";
import { fileTypesReady } from "@/mock/filetypesMock";
import "@/mock/emmetMock";
import { forgeWasm } from "@/wasm";
import { colorsApi } from "@/ipc/emmet";
import { hexOf, hsvToRgb, rgbToHsv } from "./hsv";
import { findColors, hasColors } from "./preview";

await fileTypesReady;
await forgeWasm();
await Promise.resolve();

describe("colors", () => {
  it("converts between RGB and HSB", () => {
    expect(rgbToHsv({ r: 255, g: 0, b: 0, a: 1 })).toEqual({ h: 0, s: 1, v: 1, a: 1 });
    const hsv = rgbToHsv({ r: 30, g: 144, b: 255, a: 0.5 });
    expect(Math.round(hsv.h)).toBe(210);
    expect(hexOf(hsvToRgb(hsv))).toBe("#1e90ff80");
    expect(hexOf(hsvToRgb({ h: 120, s: 1, v: 0.5, a: 1 }))).toBe("#008000");
    expect(hexOf({ r: 300, g: -4, b: 0, a: 1 })).toBe("#ff0000");
  });

  it("finds colors and writes them back in their notation through Rust", async () => {
    expect(hasColors("styles/app.scss")).toBe(true);
    expect(hasColors("README.md")).toBe(false);
    const found = await findColors("app.css", "a { color: #f00; background: rgb(0 0 255 / 50%) }");
    expect(found.map((m) => [m.text, m.hex])).toEqual([
      ["#f00", "#ff0000"],
      ["rgb(0 0 255 / 50%)", "#0000ff80"],
    ]);
    expect(await colorsApi.format({ color: { r: 0, g: 128, b: 0, a: 1 }, original: "rgb(0 0 255 / 50%)" })).toBe("rgb(0 128 0 / 100%)");
    expect(colorsApi.formatSync({ color: { r: 0, g: 0, b: 0, a: 1 }, original: "white" })).toBe("black");
    expect(await colorsApi.parse("#abc")).toMatchObject({ hex: "#aabbcc" });
    expect(colorsApi.parseSync("nope")).toBeNull();
  });
});
