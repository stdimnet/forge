/**
 * "Click and play" language servers: when a PHP project opens without Intelephense, Forge
 * offers to install it (PHPStorm-style balloon with Install / Not now / Don't ask again),
 * or installs it right away when `php.intelephense.autoInstall` is `always`. The decisions
 * live here as plain functions; `managed.ts` runs them against the real UI.
 *
 * Intelephense is never bundled with Forge (its licence forbids redistribution): the
 * backend downloads it from the npm registry onto the user's machine after they accepted
 * its licence once.
 */
import type { InstallRow, PackageUpdate, ServerRow } from "@/ipc/langservers";

export const AUTO_INSTALL_KEY = "php.intelephense.autoInstall";
export const LICENCE_ACCEPTED_KEY = "php.intelephense.licenceAccepted";
export const INTELEPHENSE_ID = "intelephense";

export type AutoInstall = "ask" | "always" | "never";

/** The install option Forge runs itself (npm packages, no prerequisites). */
export function managedOption(row: ServerRow | undefined): InstallRow | undefined {
  return row?.install.find((o) => o.managed && o.runnable);
}

export interface PromptInput {
  /** The open project uses PHP and the PHP plugin is on. */
  php: boolean;
  /** Remote projects run their servers on the host; Forge installs locally only. */
  remote: boolean;
  /** The Intelephense row of the Language Servers list. */
  row: ServerRow | undefined;
  autoInstall: string;
  /** `php.lsp.server`: phpactor or none means the user does not want Intelephense. */
  server: string;
  /** Already offered in this session (Not now). */
  offered: boolean;
}

export type PromptAction = "none" | "ask" | "install";

/** What to do about a missing Intelephense when a project opens. */
export function intelephenseAction(i: PromptInput): PromptAction {
  if (!i.php || i.remote || i.offered || !i.row) return "none";
  if (i.row.status !== "notInstalled" || i.row.path) return "none";
  if (i.server === "phpactor" || i.server === "none") return "none";
  if (!managedOption(i.row)) return "none";
  if (i.autoInstall === "never") return "none";
  return i.autoInstall === "always" ? "install" : "ask";
}

export interface BalloonAction {
  label: string;
  run: () => void;
}

export interface OfferDeps {
  showBalloon: (text: string, detail?: string, ms?: number, actions?: BalloonAction[]) => number;
  install: (row: ServerRow) => Promise<unknown>;
  saveSetting: (key: string, value: string) => Promise<void> | void;
}

export const OFFER_TEXT = "PHP language server Intelephense is not installed";
export const OFFER_DETAIL =
  "Code completion, navigation and diagnostics need it. Forge downloads it from the npm registry (and Node.js, if needed) — nothing else to set up.";

/** Run the decision: show the balloon, install, or nothing. Returns what was done. */
export function offerIntelephense(input: PromptInput, deps: OfferDeps): PromptAction {
  const action = intelephenseAction(input);
  const row = input.row;
  if (!row || action === "none") return action;
  if (action === "install") {
    void deps.install(row);
    return action;
  }
  deps.showBalloon(OFFER_TEXT, OFFER_DETAIL, 0, [
    { label: "Install", run: () => void deps.install(row) },
    { label: "Not now", run: () => {} },
    { label: "Don't ask again", run: () => void deps.saveSetting(AUTO_INSTALL_KEY, "never") },
  ]);
  return action;
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

/** "intelephense 1.12.6 → 1.14.4, yaml-language-server 1.14.0 → 1.15.0" */
export function updateSummary(updates: PackageUpdate[]): string {
  return updates.map((u) => `${u.name} ${u.installed} → ${u.latest}`).join(", ");
}
