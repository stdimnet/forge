/**
 * Repairing and resetting the IDE (`crates/forge-maintenance`, `commands/maintenance.rs`):
 * File | Invalidate Caches…, Repair IDE, and File | Manage IDE Settings (Export / Import
 * Settings, Restore Default Settings).
 */
import { invoke } from "./index";

export type CacheOption = "fileSystemAndLocalHistory" | "sharedIndexes" | "browserEngine" | "languageServerData";

export interface InvalidateModel {
  title: string;
  message: string;
  /** Shown while the Local History option is selected. */
  warning: string;
  /** What every invalidation clears. */
  always: string[];
  options: { id: CacheOption; label: string; detail: string }[];
  invalidateLabel: string;
  justRestartLabel: string;
}

/** A settings group of Export / Import Settings with the number of values it holds. */
export interface SettingsGroupCount {
  id: string;
  label: string;
  count: number;
}

export interface ExportInfo {
  groups: SettingsGroupCount[];
  defaultPath: string;
}

export const maintenanceApi = {
  invalidateModel: () => invoke<InvalidateModel>("maintenance_invalidate_model"),
  /** Schedules the cleanup for the next start (`invalidate`) and restarts Forge. */
  invalidateCaches: (options: CacheOption[], invalidate: boolean) =>
    invoke<void>("maintenance_invalidate_caches", { options, invalidate }),
  restart: () => invoke<void>("maintenance_restart"),
  /** Bring the search index in line with the disk; returns the number of changed files. */
  rescanIndexes: () => invoke<number>("maintenance_rescan_indexes"),
  exportInfo: () => invoke<ExportInfo>("settings_export_info"),
  exportSettings: (path: string, groups: string[]) => invoke<string>("settings_export", { path, groups }),
  importPreview: (path: string) => invoke<SettingsGroupCount[]>("settings_import_preview", { path }),
  /** Returns the number of imported settings and files. */
  importSettings: (path: string, groups: string[]) => invoke<number>("settings_import", { path, groups }),
  /** Backs up the configuration and resets it on the next start; returns the backup folder. */
  restoreDefaults: () => invoke<string>("settings_restore_defaults"),
};
