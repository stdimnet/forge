/** Small formatting helpers for the Browser Companion page. */

/** `482913` → `482 913`. */
export const formatPairingCode = (code: string) =>
  code.length === 6 ? `${code.slice(0, 3)} ${code.slice(3)}` : code;

/** `299` → `4:59`. */
export const formatCountdown = (secs: number) =>
  `${Math.floor(secs / 60)}:${String(Math.max(0, secs % 60)).padStart(2, "0")}`;

/** Relative time like "just now", "5 min ago", "3 days ago" (both in Unix seconds). */
export function formatAgo(then: number, now: number): string {
  const s = Math.max(0, now - then);
  if (s < 60) return "just now";
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return h === 1 ? "1 hour ago" : `${h} hours ago`;
  const d = Math.floor(h / 24);
  if (d < 30) return d === 1 ? "yesterday" : `${d} days ago`;
  return new Date(then * 1000).toLocaleDateString();
}
