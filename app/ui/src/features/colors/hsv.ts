/**
 * HSB (HSV) for the color picker's saturation/brightness square and hue slider. Parsing and
 * writing colors in the document's notation is Rust (`crates/forge-colors`); this is only
 * the geometry of the picker.
 */
import type { Rgba } from "@/ipc/emmet";

/** Hue 0..360, saturation and value 0..1, alpha 0..1. */
export interface Hsva {
  h: number;
  s: number;
  v: number;
  a: number;
}

export function rgbToHsv(c: Rgba): Hsva {
  const r = c.r / 255;
  const g = c.g / 255;
  const b = c.b / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const d = max - min;
  let h = 0;
  if (d > 1e-9) {
    if (max === r) h = ((g - b) / d) % 6;
    else if (max === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
    h = (h * 60 + 360) % 360;
  }
  return { h, s: max === 0 ? 0 : d / max, v: max, a: c.a };
}

export function hsvToRgb(c: Hsva): Rgba {
  const h = (((c.h % 360) + 360) % 360) / 60;
  const s = Math.min(1, Math.max(0, c.s));
  const v = Math.min(1, Math.max(0, c.v));
  const f = (n: number) => {
    const k = (n + h) % 6;
    return v - v * s * Math.max(0, Math.min(k, 4 - k, 1));
  };
  return { r: f(5) * 255, g: f(3) * 255, b: f(1) * 255, a: Math.min(1, Math.max(0, c.a)) };
}

const byte = (v: number) => Math.round(Math.min(255, Math.max(0, v)));

/** `#rrggbb` / `#rrggbbaa` for swatches and the hex field. */
export function hexOf(c: Rgba): string {
  const h = (v: number) => byte(v).toString(16).padStart(2, "0");
  const a = byte(c.a * 255);
  return `#${h(c.r)}${h(c.g)}${h(c.b)}${a === 255 ? "" : h(a)}`;
}

/** CSS color for drawing a color (swatches, slider ends). */
export const cssOf = (c: Rgba): string => `rgba(${byte(c.r)}, ${byte(c.g)}, ${byte(c.b)}, ${Math.round(c.a * 1000) / 1000})`;
