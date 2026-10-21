/** Text of the update panel's download rows (the numbers come from Rust). */
import type { DownloadPhase, DownloadProgress } from "@/ipc/updates";

const MB = 1024 * 1024;

/** `12 / 94 MB` */
export function formatTransferred(downloaded: number, total?: number | null): string {
  const done = Math.round(downloaded / MB);
  return total ? `${done} / ${Math.round(total / MB)} MB` : `${done} MB`;
}

/** `9.8 MB/s`, empty while unknown. */
export function formatSpeed(bytesPerSecond: number): string {
  return bytesPerSecond > 0 ? `${(bytesPerSecond / MB).toFixed(1)} MB/s` : "";
}

/** 0–1 */
export function downloadFraction(p: DownloadProgress | null): number {
  if (!p) return 0;
  if (p.phase !== "downloading" && p.phase !== "failed") return 1;
  return p.total ? Math.min(1, p.downloaded / p.total) : 0;
}

/** The download is over (the startup continues or Forge restarts). */
export const isFinished = (phase: DownloadPhase | undefined) =>
  phase === "simulated" || phase === "failed" || phase === "restarting";

/** First row, left: what is happening. */
export function downloadLabel(version: string, p: DownloadProgress | null): string {
  switch (p?.phase) {
    case "installing":
      return `Installing ${version}`;
    case "restarting":
      return "Restarting…";
    case "simulated":
      return "Simulated update finished";
    case "failed":
      return `Update to ${version} failed`;
    default:
      return `Downloading ${version}`;
  }
}

/** Second row, left: the signature check. */
export function signatureLabel(p: DownloadProgress | null): { text: string; tone: "muted" | "ok" | "error" } {
  switch (p?.phase) {
    case "verifying":
      return { text: "Signature: verifying…", tone: "muted" };
    case "verified":
    case "installing":
    case "restarting":
      return { text: "Signature: valid", tone: "ok" };
    case "simulated":
      return { text: "Signature: valid (simulated)", tone: "ok" };
    case "failed":
      return { text: p.message ?? "The update failed.", tone: "error" };
    default:
      return { text: "Signature: waiting", tone: "muted" };
  }
}
