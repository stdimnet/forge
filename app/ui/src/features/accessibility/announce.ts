/**
 * Screen reader announcements: two visually hidden live regions (polite and assertive) that
 * exist from startup, so assistive technology picks up every later change. Features call
 * `announce` for things a screen reader would otherwise miss; `announceForScreenReader` only
 * speaks when "Support screen readers" (Settings | Appearance & Behavior | Appearance) is on.
 */
import { getSetting } from "@/state/settings";

export type Politeness = "polite" | "assertive";

const regions = new Map<Politeness, HTMLElement>();

/** The live region of a politeness level, created on first use. */
export function liveRegion(level: Politeness = "polite", doc: Document = document): HTMLElement {
  const existing = regions.get(level);
  if (existing?.isConnected && existing.ownerDocument === doc) return existing;
  const el = doc.createElement("div");
  el.className = "sr-only";
  el.dataset.liveRegion = level;
  el.setAttribute("aria-live", level);
  el.setAttribute("aria-atomic", "true");
  el.setAttribute("role", level === "assertive" ? "alert" : "status");
  doc.body.appendChild(el);
  regions.set(level, el);
  return el;
}

const seqs: Record<Politeness, number> = { polite: 0, assertive: 0 };

/**
 * Speak `text` through the screen reader. The region is cleared first and filled a moment
 * later, so the same text twice in a row is announced twice.
 */
export function announce(text: string, level: Politeness = "polite"): void {
  if (typeof document === "undefined" || !text.trim()) return;
  const el = liveRegion(level);
  const n = ++seqs[level];
  el.textContent = "";
  setTimeout(() => {
    if (n === seqs[level]) el.textContent = text;
  }, 30);
}

/** Whether "Support screen readers" is on. */
export const screenReaderMode = (): boolean => getSetting<boolean>("appearance.screenReader", false) === true;

/** `announce`, but only with "Support screen readers" on (extra announcements). */
export function announceForScreenReader(text: string, level: Politeness = "polite"): void {
  if (screenReaderMode()) announce(text, level);
}
