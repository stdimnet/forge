/**
 * Browser-mode Invalidate Caches, Repair IDE and Manage IDE Settings. The dialog models and
 * the settings groups come from the same Rust code as WebAssembly (`forge-maintenance`);
 * nothing is deleted, archived or restarted here.
 */
import { registerMock } from "@/ipc";
import type { CacheOption, InvalidateModel, SettingsGroupCount } from "@/ipc/maintenance";
import { callWasmSync, forgeWasm } from "@/wasm";
import { mockSettingLayers } from "./settingsMock";

/** What the last "Invalidate and Restart" would have cleaned (for tests). */
export let scheduledInvalidation: CacheOption[] | null = null;

const groups = async (keys: string[]): Promise<SettingsGroupCount[]> =>
  callWasmSync<SettingsGroupCount[]>((await forgeWasm()).maintenance_settings_groups, { keys, files: [] });

/** Keys of a settings archive someone exported (Import Settings in browser mode). */
const DEMO_ARCHIVE = ["editor.font.size", "editor.tabs.limit", "keymap.active", "fileTypes.associations", "php.languageLevel"];

registerMock("maintenance_invalidate_model", async () => JSON.parse((await forgeWasm()).maintenance_invalidate_model()) as InvalidateModel);
registerMock("maintenance_invalidate_caches", ({ options, invalidate }) => {
  if (invalidate) scheduledInvalidation = (options as CacheOption[]) ?? [];
});
registerMock("maintenance_restart", () => undefined);
registerMock("maintenance_rescan_indexes", () => 0);
registerMock("settings_export_info", async () => ({
  groups: await groups(Object.keys(mockSettingLayers().user)),
  defaultPath: "~/settings.zip",
}));
registerMock("settings_export", ({ path }) => String(path));
registerMock("settings_import_preview", () => groups(DEMO_ARCHIVE));
registerMock("settings_import", ({ groups }) => (groups as string[]).length);
registerMock("settings_restore_defaults", () => "~/.config/forge-backup-20261023-142501");
