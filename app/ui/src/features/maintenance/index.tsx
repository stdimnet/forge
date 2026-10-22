/**
 * Repairing and resetting the IDE, like PHPStorm's File menu:
 * - File | Invalidate Caches… (`InvalidateCaches`) and File | Repair IDE (`CallSaul`);
 * - File | Reload All from Disk (`Synchronize`), Repair IDE's first step;
 * - File | Manage IDE Settings: Import Settings… (`ImportSettings`), Export Settings…
 *   (`ExportSettings`) and Restore Default Settings… (`RestoreDefaultSettings`).
 * The models and the work live in Rust (`crates/forge-maintenance`).
 */
import { isTauri } from "@/ipc";
import { balloonsFor } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { registerDialog, openDialogWindow } from "@/shell/window/dialogs";
import { askChoice } from "@/features/contextMenu/dialogs";
import { maintenanceApi } from "@/ipc/maintenance";
import { INVALIDATE_DIALOG, InvalidateCachesDialog, type InvalidateResult } from "./InvalidateCachesDialog";
import { SettingsTransferDialog, TRANSFER_DIALOG, type TransferProps, type TransferResult } from "./SettingsTransferDialog";
import { refreshProjectFiles, startRepair } from "./repair";
import "./maintenance.css";

const showBalloon = balloonsFor("IDE Settings");

const message = (e: unknown) => String(e instanceof Error ? e.message : e);

/** Restart Forge; browser mode only says so (it cannot restart). */
async function restart(): Promise<void> {
  if (!isTauri()) {
    showBalloon("Forge would restart now", "Restarting is not available in browser mode.");
    return;
  }
  await maintenanceApi.restart();
}

registerDialog<Record<string, never>, InvalidateResult>({
  id: INVALIDATE_DIALOG,
  title: "Invalidate Caches",
  width: 540,
  height: 400,
  minWidth: 440,
  minHeight: 320,
  component: InvalidateCachesDialog,
  onClosed: (result) => {
    if (!result) return;
    if (!isTauri()) {
      const what = result.invalidate ? "Caches will be invalidated on the next start" : "Forge would restart now";
      showBalloon(what, "Restarting is not available in browser mode.");
    }
    void maintenanceApi.invalidateCaches(result.options, result.invalidate).catch((e) => showBalloon("Invalidate Caches failed", message(e), 6000, undefined, { type: "error" }));
  },
});

registerDialog<TransferProps, TransferResult>({
  id: TRANSFER_DIALOG,
  title: (props) => (props?.mode === "import" ? "Import Settings" : "Export Settings"),
  width: 520,
  height: 560,
  minWidth: 420,
  minHeight: 380,
  component: SettingsTransferDialog,
  onClosed: (result, props) => {
    if (!result) return;
    if (props.mode === "export") void exportTo(result);
    else void importFrom(result);
  },
});

async function exportTo(result: TransferResult): Promise<void> {
  try {
    const path = await maintenanceApi.exportSettings(result.path, result.groups);
    showBalloon("Settings exported", path);
  } catch (e) {
    showBalloon("Export failed", message(e), 6000, undefined, { type: "error" });
  }
}

async function importFrom(result: TransferResult): Promise<void> {
  try {
    await maintenanceApi.importSettings(result.path, result.groups);
  } catch (e) {
    showBalloon("Import failed", message(e), 6000, undefined, { type: "error" });
    return;
  }
  // Like PHPStorm, the imported settings take effect after a restart.
  await restart();
}

async function exportSettings(): Promise<void> {
  const info = await maintenanceApi.exportInfo();
  await openDialogWindow(TRANSFER_DIALOG, { mode: "export", groups: info.groups, path: info.defaultPath } satisfies TransferProps);
}

async function importSettings(): Promise<void> {
  let path = "settings.zip";
  if (isTauri()) {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({ multiple: false, title: "Import Settings", filters: [{ name: "Settings archive", extensions: ["zip"] }] });
    if (typeof picked !== "string") return;
    path = picked;
  }
  try {
    const groups = await maintenanceApi.importPreview(path);
    await openDialogWindow(TRANSFER_DIALOG, { mode: "import", groups, path } satisfies TransferProps);
  } catch (e) {
    showBalloon("Import failed", message(e), 6000, undefined, { type: "error" });
  }
}

async function restoreDefaultSettings(): Promise<void> {
  const answer = await askChoice({
    title: "Restore Default Settings",
    message: "Forge will back up your current settings and restart with the default settings. Connections, accounts and recent projects are kept.",
    choices: [{ id: "restore", label: "Restore and Restart" }],
  });
  if (answer !== "restore") return;
  try {
    const backup = await maintenanceApi.restoreDefaults();
    showBalloon("Settings were backed up", backup, 6000);
  } catch (e) {
    showBalloon("Restore Default Settings failed", message(e), 6000, undefined, { type: "error" });
    return;
  }
  await restart();
}

registerCommand({ id: "InvalidateCaches", title: "Invalidate Caches…", category: "File", run: () => void openDialogWindow(INVALIDATE_DIALOG, {}) });
registerCommand({ id: "CallSaul", title: "Repair IDE", category: "File", run: () => void startRepair() });
registerCommand({ id: "Synchronize", title: "Reload All from Disk", category: "File", run: refreshProjectFiles });
registerCommand({ id: "ExportSettings", title: "Export Settings…", category: "File", run: () => void exportSettings().catch((e) => showBalloon("Export failed", message(e))) });
registerCommand({ id: "ImportSettings", title: "Import Settings…", category: "File", run: () => void importSettings() });
registerCommand({ id: "RestoreDefaultSettings", title: "Restore Default Settings…", category: "File", run: () => void restoreDefaultSettings() });
