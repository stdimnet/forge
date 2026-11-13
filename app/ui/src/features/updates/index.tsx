/**
 * Updates in the workbench: Help | Check for Updates…, the Updates settings page, and the
 * offer of an update found during startup when the splash window is turned off. The splash
 * itself lives in `src/splash/`.
 */
import { render } from "solid-js/web";
import { isTauri } from "@/ipc";
import { startupApi } from "@/ipc/updates";
import { registerCommand } from "@/state/commands";
import { registerSettingsPage } from "@/features/settingsPages";
import { openCheckForUpdates, openUpdateOffer, UpdateDialogHost } from "./UpdateDialog";
import { UpdatesSettingsPage } from "./UpdatesSettingsPage";
import { t } from "@/i18n";

export { openCheckForUpdates } from "./UpdateDialog";

registerCommand({
  id: "CheckForUpdate",
  title: "Check for Updates…",
  category: "Help",
  run: () => void openCheckForUpdates(),
});

registerSettingsPage({
  id: "system.updates",
  title: t("settings.page.system.updates"),
  group: "Appearance & Behavior",
  parent: "system",
  order: 0,
  keywords: ["update", "upgrade", "release", "channel", "version", "splash"],
  component: UpdatesSettingsPage,
});

/** The main window has rendered: Rust shows it and closes the splash window. */
export async function reportWorkbenchReady(): Promise<void> {
  if (!isTauri()) return;
  try {
    const offer = await startupApi.workbenchReady();
    if (offer) openUpdateOffer(offer);
  } catch (e) {
    console.warn("cannot report the workbench as ready:", e);
  }
}

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "update-dialog-host";
  document.body.appendChild(host);
  render(() => <UpdateDialogHost />, host);
}
