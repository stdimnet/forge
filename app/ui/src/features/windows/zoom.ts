/**
 * View | Appearance | Zoom IDE: the steps of `appearance.zoom` (Settings | Appearance |
 * Zoom), like PHPStorm's IDE scale. The value is a percentage; the whole UI is scaled (the
 * webview zoom in the desktop app, CSS `zoom` in browser mode).
 */
export const ZOOM_STEPS = [50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300];
export const DEFAULT_ZOOM = 100;

/** The setting value as a percentage (unknown values count as 100 %). */
export function zoomPercent(value: unknown): number {
  const n = Number(value);
  return Number.isFinite(n) && n >= ZOOM_STEPS[0] && n <= ZOOM_STEPS[ZOOM_STEPS.length - 1] ? n : DEFAULT_ZOOM;
}

/** The next step up (`1`) or down (`-1`) from `current`; stays at the ends. */
export function nextZoom(current: number, direction: 1 | -1): number {
  if (direction > 0) return ZOOM_STEPS.find((s) => s > current) ?? ZOOM_STEPS[ZOOM_STEPS.length - 1];
  return [...ZOOM_STEPS].reverse().find((s) => s < current) ?? ZOOM_STEPS[0];
}
