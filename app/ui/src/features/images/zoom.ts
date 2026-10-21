/** Zoom steps and the info line of the image editor. */
import type { ImageInfo } from "@/ipc/viewers";

const STEPS = [0.05, 0.1, 0.125, 0.25, 0.33, 0.5, 0.67, 0.75, 1, 1.5, 2, 3, 4, 6, 8, 12, 16, 24, 32];

/** The next zoom step up (`dir` 1) or down (-1) from `zoom`. */
export function nextZoom(zoom: number, dir: 1 | -1): number {
  if (dir > 0) return STEPS.find((s) => s > zoom + 1e-6) ?? STEPS[STEPS.length - 1];
  return [...STEPS].reverse().find((s) => s < zoom - 1e-6) ?? STEPS[0];
}

/** The zoom that fits a `w`x`h` image into `maxW`x`maxH`. */
export function fitZoom(w: number, h: number, maxW: number, maxH: number): number {
  if (w <= 0 || h <= 0 || maxW <= 0 || maxH <= 0) return 1;
  return Math.max(STEPS[0], Math.min(maxW / w, maxH / h));
}

/** "1.4 kB", "512 B", "2.1 MB" (decimal units, like PHPStorm). */
export function formatSize(bytes: number): string {
  if (bytes < 1000) return `${bytes} B`;
  if (bytes < 1_000_000) return `${(bytes / 1000).toFixed(bytes < 10_000 ? 2 : 1)} kB`;
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}

/** "128x128 PNG (32-bit color) 484 B". */
export function formatImageInfo(info: ImageInfo): string {
  const depth = info.bitDepth ? ` (${info.bitDepth}-bit color)` : "";
  return `${info.width}x${info.height} ${info.format}${depth} ${formatSize(info.size)}`;
}
