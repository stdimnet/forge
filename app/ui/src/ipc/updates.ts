/**
 * Startup splash and updates (`app/src-tauri/src/startup.rs`, `src/commands/updates.rs`;
 * decisions in `crates/forge-update`). Browser mode: `mock/updatesMock.ts`.
 */
import { invoke, listen } from "./index";

/** One loading step of the splash: `splash://progress`. */
export interface StartupProgress {
  step: string;
  detail: string;
  /** 0–100: how far the wordmark is filled. */
  percent: number;
}

export interface UpdateOffer {
  version: string;
  currentVersion: string;
  notes?: string;
  date?: string;
  /** Download size in bytes. */
  size?: number;
  /** "You have 0.1.0. The download is 94 MB and is checked against …" */
  summary: string;
  /** Simulated (`FORGE_SIMULATE_UPDATE`, `?simulateUpdate`): nothing is installed. */
  simulated: boolean;
  /** The feed the offer came from (the beta channel reads two). */
  feed?: string;
}

export interface SplashSnapshot {
  progress: StartupProgress;
  version: string;
  theme: string;
  offer: UpdateOffer | null;
}

export type DownloadPhase =
  | "downloading"
  | "verifying"
  | "verified"
  | "installing"
  | "restarting"
  | "simulated"
  | "failed";

/** `updates://download` */
export interface DownloadProgress {
  version: string;
  phase: DownloadPhase;
  downloaded: number;
  total?: number | null;
  bytesPerSecond: number;
  message?: string;
}

export type SkipReason =
  | { kind: "devBuild" }
  | { kind: "notConfigured" }
  | { kind: "disabled" }
  | { kind: "checkedRecently"; nextInSecs: number };

export type CheckResult =
  | { status: "available"; offer: UpdateOffer }
  | { status: "upToDate"; version: string }
  | { status: "notChecked"; reason: SkipReason; message: string }
  | { status: "error"; message: string };

export type UpdateMode = "enabled" | "simulated" | "devBuild" | "notConfigured";

export interface UpdatesStatus {
  mode: UpdateMode;
  currentVersion: string;
  endpoint: string;
  /** Unix seconds. */
  lastCheck?: number | null;
  skippedVersion?: string | null;
}

export type UpdateAction = "download" | "later" | "skip";

export const startupApi = {
  /** The splash window's state when it has loaded. */
  splashState: () => invoke<SplashSnapshot>("splash_state"),
  /** The main window rendered: Rust shows it and closes the splash. */
  workbenchReady: () => invoke<UpdateOffer | null>("startup_workbench_ready"),
  onProgress: (cb: (p: StartupProgress) => void) => listen<StartupProgress>("splash://progress", cb),
  onOffer: (cb: (o: UpdateOffer) => void) => listen<UpdateOffer>("splash://update", cb),
};

export const updatesApi = {
  /** Help | Check for Updates…: ignores the daily limit and skipped versions. */
  check: () => invoke<CheckResult>("updates_check"),
  respond: (action: UpdateAction) => invoke<void>("updates_respond", { action }),
  status: () => invoke<UpdatesStatus>("updates_status"),
  clearSkipped: () => invoke<void>("updates_clear_skipped"),
  onDownload: (cb: (p: DownloadProgress) => void) => listen<DownloadProgress>("updates://download", cb),
};
