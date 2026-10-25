/**
 * Project trust (PHPStorm's "Trust and Open Project?" and Safe Mode): the dialog before an
 * untrusted project opens, the Safe Mode banner in the editor area and Settings | Build,
 * Execution, Deployment | Trusted Locations. Rust decides and enforces (`forge-trust`); the
 * UI only asks and shows.
 */
import { Show } from "solid-js";
import { ShieldAlert } from "lucide-solid";
import { trustApi } from "@/ipc/trust";
import { showBalloon } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { setTrustPrompt, type TrustDecision } from "@/state/workspace";
import { registerSettingsPage } from "@/features/settingsPages";
import { askTrust } from "./TrustDialog";
import { refreshTrust, safeMode, trustStatus } from "./store";
import { TrustedLocationsPage } from "./TrustedLocationsPage";
import "./trust.css";

export { safeMode, trustStatus };

async function decide(check: Parameters<typeof askTrust>[0], path?: string): Promise<TrustDecision> {
  const answer = await askTrust(check, "open");
  if (answer.decision !== "trust") return answer.decision;
  try {
    await trustApi.trust(path, answer.trustLocation ? check.parent : undefined);
  } catch (e) {
    showBalloon("Cannot trust the project", String(e));
    return "safe";
  }
  return "trust";
}

setTrustPrompt({
  async beforeOpen(path) {
    const check = await trustApi.check(path).catch(() => null);
    if (!check || check.trusted) return "trust";
    return decide(check, path);
  },
  async afterRemoteOpen() {
    const status = await trustApi.status().catch(() => null);
    if (!status || status.trusted) return "trust";
    const decision = await decide(status);
    await refreshTrust();
    return decision;
  },
});

/** "Trust Project…" in the banner: trust the open project (and maybe its folder). */
export async function trustOpenProject(): Promise<void> {
  const status = trustStatus();
  if (!status || status.trusted) return;
  const answer = await askTrust(status, "trust");
  if (answer.decision !== "trust") return;
  try {
    await trustApi.trust(undefined, answer.trustLocation ? status.parent : undefined);
    await refreshTrust();
    showBalloon("Project trusted", "Scripts, run configurations and tools of the project can run now.");
  } catch (e) {
    showBalloon("Cannot trust the project", String(e));
  }
}

registerCommand({
  id: "forge.trustProject",
  title: "Trust Project…",
  category: "File",
  run: () => void trustOpenProject(),
});

/** The banner above the editors while the project is in Safe Mode. */
export function SafeModeBanner() {
  return (
    <Show when={safeMode()}>
      <div class="safe-mode-banner" role="status" data-testid="safe-mode-banner">
        <ShieldAlert size={14} class="safe-mode-banner-icon" />
        <span class="safe-mode-banner-text">
          <strong>Safe mode, limited functionality.</strong> Trust the project to run its scripts, run configurations,
          tests and tools.
        </span>
        <button class="safe-mode-banner-link" onClick={() => void trustOpenProject()} data-testid="safe-mode-trust">
          Trust Project…
        </button>
      </div>
    </Show>
  );
}

registerSettingsPage({
  id: "build.trusted-locations",
  title: "Trusted Locations",
  group: "Build, Execution, Deployment",
  order: 40,
  keywords: ["trust", "safe mode", "trusted", "untrusted", "security", "location"],
  component: TrustedLocationsPage,
});
