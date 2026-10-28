/**
 * Helpers for Forge-managed installs of language servers (npm packages and Node.js
 * downloaded and verified by the backend): the install option Forge runs itself, progress
 * text for the status bar, update summaries and failure details for balloons. Plain
 * functions; `managed.ts` runs them against the real UI.
 */
import type { InstallRow, PackageUpdate, ServerRow } from "@/ipc/langservers";

/** The install option Forge runs itself (npm packages, no prerequisites). */
export function managedOption(row: ServerRow | undefined): InstallRow | undefined {
  return row?.install.find((o) => o.managed && o.runnable);
}

/** "Downloading Node.js v22.11.0 · 45%" for the status bar. */
export function progressText(label: string, done: number, total?: number | null): string {
  if (total && total > 0) return `${label} · ${Math.min(100, Math.floor((done * 100) / total))}%`;
  if (done > 0) return `${label} · ${(done / 1048576).toFixed(1)} MB`;
  return label;
}

/** Fraction for the progress bar, or null while the size is unknown. */
export function progressFraction(done: number, total?: number | null): number | null {
  return total && total > 0 ? Math.min(1, done / total) : null;
}

/** "typescript-language-server 4.3.3 → 4.3.4, yaml-language-server 1.14.0 → 1.15.0" */
export function updateSummary(updates: PackageUpdate[]): string {
  return updates.map((u) => `${u.name} ${u.installed} → ${u.latest}`).join(", ");
}

/**
 * Why an install failed, for the balloon: the last error line the installer printed (npm's
 * `npm error` prefix removed), and the exit code.
 */
export function failureDetail(code: number | null | undefined, stderr: string[]): string {
  const lines = stderr.map((l) => l.replace(/^npm (error|ERR!)\s*/, "").trim()).filter(Boolean);
  const reason = [...lines].reverse().find((l) => /fail|error|denied|not found|certificate|ENOTFOUND|ETIMEDOUT/i.test(l) && !/complete log/i.test(l)) ?? lines.at(-1);
  const exit = code == null ? "" : `Exit code ${code}.`;
  const sentence = reason && !/[.!?]$/.test(reason) ? `${reason}.` : reason;
  return [sentence, exit].filter(Boolean).join(" ") || "See Settings | Languages & Frameworks | Language Servers.";
}
