import type { SearchStatus } from "@/ipc/search";

export const count = (n: number) => n.toLocaleString("en-US");

export function indexTooltip(s: SearchStatus, now = Date.now()): string {
  const lines = [`Search index: ${count(s.files)} files`];
  lines.push(`${count(s.contentFiles)} searched by content`);
  if (s.binaryFiles) lines.push(`${count(s.binaryFiles)} binary, by name only`);
  if (s.oversizedFiles) lines.push(`${count(s.oversizedFiles)} over the size limit, by name only`);
  if (s.indexing) lines.push("Updating…");
  else if (s.lastRefreshMs) lines.push(`Updated ${Math.max(0, Math.round((now - s.lastRefreshMs) / 1000))} s ago`);
  return lines.join("\n");
}

