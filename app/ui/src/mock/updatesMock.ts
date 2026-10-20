/**
 * Browser-mode startup and updates. `?splash` previews the splash window: the loading steps
 * of `app/src-tauri/src/startup.rs` play with the demo project, and `?simulateUpdate=0.2.0`
 * offers a simulated update with a fake download, like `FORGE_SIMULATE_UPDATE` in desktop
 * dev builds. Browser mode never checks for real updates.
 *
 * Screenshot aid: `?splashPause=40` stops the loading steps once the next one would pass 40 %.
 */
import { emitMock, registerMock } from "@/ipc";
import type {
  CheckResult,
  DownloadPhase,
  DownloadProgress,
  SplashSnapshot,
  StartupProgress,
  UpdateAction,
  UpdateOffer,
  UpdatesStatus,
} from "@/ipc/updates";
import { version as uiVersion } from "../../package.json";
import { demoProjectName } from "./demoProject";

const MB = 1024 * 1024;
const SIMULATED_SIZE = 94 * MB;
const DEFAULT_ENDPOINT = "https://github.com/Reeperk/Forge/releases/latest/download/latest.json";

const params = () => new URLSearchParams(typeof location === "undefined" ? "" : location.search);
const simulateVersion = () => params().get("simulateUpdate")?.trim().replace(/^v/, "") || null;
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

let progress: StartupProgress = { step: "Starting", detail: "", percent: 0 };
let pending: UpdateOffer | null = null;
let waiting: (() => void) | null = null;
let started = false;
let skipped: string | null = null;

function report(step: string, detail: string, percent: number) {
  progress = { step, detail, percent };
  emitMock("splash://progress", progress);
}

function simulatedOffer(version: string): UpdateOffer {
  return {
    version,
    currentVersion: uiVersion,
    notes: "Simulated update for development.",
    size: SIMULATED_SIZE,
    summary: `You have ${uiVersion}. Simulated update: a fake 94 MB download, nothing is installed.`,
    simulated: true,
  };
}

/** The steps of `startup::run_steps`, with the demo project. */
async function runSplash() {
  const pause = Number(params().get("splashPause") ?? "NaN");
  const steps: [string, string, number][] = [
    ["Loading the core", "settings · keymap", 4],
    ["Loading plugins", "36 plugins", 16],
    ["Opening project", demoProjectName, 38],
    ["Opening project", `${demoProjectName} · Laravel`, 50],
    ["Starting background services", "language servers", 56],
    ["Starting background services", "file watcher", 62],
    ["Starting background services", "Docker · Kubernetes", 70],
    ["Checking for updates", simulateVersion() ? "simulated" : "development build", 78],
  ];
  for (const [step, detail, percent] of steps) {
    await sleep(450);
    if (percent > pause) return;
    report(step, detail, percent);
  }
  await sleep(450);
  const version = simulateVersion();
  if (version) {
    pending = simulatedOffer(version);
    report("Update available", version, 88);
    emitMock("splash://update", pending);
    await new Promise<void>((resolve) => (waiting = resolve));
  } else {
    report("Checking for updates", "development build", 86);
  }
  await sleep(350);
  report("Opening workbench", "", 92);
  await sleep(500);
  report("Ready", "", 100);
}

function emitDownload(phase: DownloadPhase, downloaded: number, bytesPerSecond: number, message?: string) {
  const p: DownloadProgress = {
    version: pending?.version ?? "",
    phase,
    downloaded,
    total: SIMULATED_SIZE,
    bytesPerSecond,
    message,
  };
  emitMock("updates://download", p);
}

/** The fake download of `forge_update::simulate`. */
async function simulateDownload() {
  let sent = 0;
  for (let step = 0; sent < SIMULATED_SIZE; step++) {
    await sleep(170);
    sent = Math.min(SIMULATED_SIZE, sent + (6 + ((step * 5) % 8)) * MB);
    emitDownload("downloading", sent, Math.round((9 + (step % 4)) * MB));
  }
  emitDownload("verifying", sent, 0);
  await sleep(700);
  emitDownload("verified", sent, 0);
  await sleep(800);
  emitDownload("simulated", sent, 0, "Simulated update: nothing was installed.");
  await sleep(1500);
  pending = null;
  waiting?.();
  waiting = null;
}

registerMock("splash_state", (): SplashSnapshot => {
  if (!started) {
    started = true;
    void runSplash();
  }
  return { progress, version: uiVersion, theme: document.documentElement.dataset.theme ?? "dark", offer: pending };
});

registerMock("startup_workbench_ready", () => null);

registerMock("updates_check", async (): Promise<CheckResult> => {
  await sleep(600);
  const version = simulateVersion();
  if (version) {
    pending = simulatedOffer(version);
    return { status: "available", offer: pending };
  }
  return {
    status: "notChecked",
    reason: { kind: "devBuild" },
    message: "Browser mode does not check for updates. Add ?simulateUpdate=0.2.0 to the URL to try the update flow.",
  };
});

registerMock("updates_respond", ({ action }) => {
  if (!pending) throw new Error("no update is pending");
  switch (action as UpdateAction) {
    case "download":
      void simulateDownload();
      return;
    case "later":
      report("Update postponed", "asks again tomorrow", 90);
      break;
    case "skip":
      skipped = pending.version;
      report(`Skipped ${pending.version}`, "no reminders for this version", 90);
      break;
  }
  pending = null;
  waiting?.();
  waiting = null;
});

registerMock(
  "updates_status",
  (): UpdatesStatus => ({
    mode: simulateVersion() ? "simulated" : "devBuild",
    currentVersion: uiVersion,
    endpoint: DEFAULT_ENDPOINT,
    lastCheck: null,
    skippedVersion: skipped,
  }),
);

registerMock("updates_clear_skipped", () => {
  skipped = null;
});
