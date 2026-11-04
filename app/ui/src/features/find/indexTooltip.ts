import type { SearchStatus } from "@/ipc/search";
import { t } from "@/i18n";

export const count = (n: number) => n.toLocaleString("en-US");

export function indexTooltip(s: SearchStatus, now = Date.now()): string {
  const lines = [t("index.tooltip.files", { count: s.files })];
  lines.push(t("index.tooltip.content", { count: s.contentFiles }));
  if (s.binaryFiles) lines.push(t("index.tooltip.binary", { count: s.binaryFiles }));
  if (s.oversizedFiles) lines.push(t("index.tooltip.oversized", { count: s.oversizedFiles }));
  if (s.indexing) lines.push(t("index.tooltip.updating"));
  else if (s.lastRefreshMs) lines.push(t("index.tooltip.updated", { seconds: Math.max(0, Math.round((now - s.lastRefreshMs) / 1000)) }));
  return lines.join("\n");
}

