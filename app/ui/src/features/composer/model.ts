/** Small helpers of the Composer UI that build Composer command lines. */
import type { OutdatedMarker } from "@/ipc/composer";
import { requireCommand } from "./state";

/**
 * The command of "Update package": `update <name> --with-dependencies` when the constraint
 * allows the newest version, else `require <name>:<new constraint>` (which rewrites the
 * constraint in composer.json, like PHPStorm's quick fix).
 */
export function updateCommand(m: Pick<OutdatedMarker, "name" | "status" | "suggested" | "dev">): string {
  return m.status === "semver-safe-update" ? `update ${m.name} --with-dependencies` : requireCommand(m.name, m.suggested, m.dev);
}

/** Human-readable download count (Packagist statistics): 1.2M, 34K. */
export function formatDownloads(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(n >= 10_000_000 ? 0 : 1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(n >= 10_000 ? 0 : 1)}K`;
  return String(n);
}
