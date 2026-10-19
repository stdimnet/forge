/** Date helpers matching PHPStorm's wording ("2 days ago", "Today 14:03"). */

export function timeAgo(unixSeconds: number, now = Date.now()): string {
  const s = Math.max(0, Math.round(now / 1000 - unixSeconds));
  const units: [number, string][] = [
    [365 * 86400, "year"],
    [30 * 86400, "month"],
    [7 * 86400, "week"],
    [86400, "day"],
    [3600, "hour"],
    [60, "minute"],
  ];
  for (const [size, name] of units) {
    const n = Math.floor(s / size);
    if (n >= 1) return `${n} ${name}${n > 1 ? "s" : ""} ago`;
  }
  return "moments ago";
}

const pad = (n: number) => String(n).padStart(2, "0");

/** Short date for blame annotations: `12.10.26`. */
export function shortDate(unixSeconds: number): string {
  const d = new Date(unixSeconds * 1000);
  return `${pad(d.getDate())}.${pad(d.getMonth() + 1)}.${String(d.getFullYear()).slice(2)}`;
}

/** Log column date: "Today 14:03", "Yesterday 09:12", else `12.10.26, 14:03`. */
export function logDate(unixSeconds: number, now = new Date()): string {
  const d = new Date(unixSeconds * 1000);
  const time = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((day(now) - day(d)) / 86400000);
  if (diff === 0) return `Today ${time}`;
  if (diff === 1) return `Yesterday ${time}`;
  return `${shortDate(unixSeconds)}, ${time}`;
}

export function fullDate(unixSeconds: number): string {
  const d = new Date(unixSeconds * 1000);
  return `${pad(d.getDate())}.${pad(d.getMonth() + 1)}.${d.getFullYear()}, ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** Age bucket 0 (newest) .. 4 (oldest) relative to the file's commit time range. */
export function ageBucket(time: number, min: number, max: number): number {
  if (max <= min) return 0;
  const r = (max - time) / (max - min);
  return Math.min(4, Math.floor(r * 5));
}
