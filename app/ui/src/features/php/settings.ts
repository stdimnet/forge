/**
 * PHP settings (Settings > Languages & Frameworks > PHP), stored by the settings backend
 * under the keys declared in `app/src-tauri/plugins/php.toml`. The backend reads the same
 * keys to configure (and restart) the PHP language server.
 */
import { createEffect, createRoot, on } from "solid-js";
import { getSetting, saveSetting, saveSettings, settingsState } from "@/state/settings";
import type { SettingValue } from "@/ipc/settings";
import type { PhpLspSettings } from "./api";

export interface PhpSettings {
  /** PHP language level, e.g. "8.3". */
  languageLevel: string;
  /** CLI interpreter executable (`php`, `/usr/bin/php8.3`, `docker://...`). */
  cliInterpreter: string;
  includePaths: string[];
}

export const LANGUAGE_LEVELS = ["7.4", "8.0", "8.1", "8.2", "8.3", "8.4"] as const;

/** `php.languageLevel` value: the level follows the CLI interpreter's version. */
export const LEVEL_FROM_INTERPRETER = "interpreter";

/** Settings key of each field. */
export const PHP_SETTING_KEYS: Record<keyof PhpSettings, string> = {
  languageLevel: "php.languageLevel",
  cliInterpreter: "php.interpreter",
  includePaths: "php.includePaths",
};

export const defaultPhpSettings: PhpSettings = {
  languageLevel: "8.3",
  cliInterpreter: "php",
  includePaths: [],
};

/** Current PHP settings (reactive: reading a field inside an effect tracks it). */
export const phpSettings: Readonly<PhpSettings> = {
  get languageLevel() {
    return getSetting<string>(PHP_SETTING_KEYS.languageLevel, defaultPhpSettings.languageLevel);
  },
  get cliInterpreter() {
    return getSetting<string>(PHP_SETTING_KEYS.cliInterpreter, defaultPhpSettings.cliInterpreter);
  },
  get includePaths() {
    return getSetting<string[]>(PHP_SETTING_KEYS.includePaths, defaultPhpSettings.includePaths);
  },
};

/** Persist changed fields (project-level keys go to the open project). */
export function updatePhpSettings(patch: Partial<PhpSettings>): Promise<void> {
  const changes: Record<string, SettingValue> = {};
  for (const [field, value] of Object.entries(patch) as [keyof PhpSettings, SettingValue][]) {
    changes[PHP_SETTING_KEYS[field]] = value;
  }
  return saveSettings(changes);
}

/** The backend's `PhpLspSettings`. */
export function toLspSettings(s: PhpSettings = phpSettings): PhpLspSettings {
  return {
    phpVersion: s.languageLevel,
    maxFileSize: 5_000_000,
    extraStubs: [],
    includePaths: [...s.includePaths],
  };
}

/** Where earlier versions kept PHP settings in the browser storage. */
export const LEGACY_STORAGE_KEY = "forge.php.settings";

/**
 * Settings to write for values saved by earlier versions in localStorage. Values the
 * settings backend already has (`hasValue`) win. The language server choice and the
 * Intelephense licence key of those versions are dropped: php-lsp is the only PHP server.
 */
export function legacyPhpChanges(raw: string | null, hasValue: (key: string) => boolean): Record<string, SettingValue> {
  if (!raw) return {};
  let old: Record<string, unknown>;
  try {
    old = JSON.parse(raw) as Record<string, unknown>;
  } catch {
    return {};
  }
  const out: Record<string, SettingValue> = {};
  const take = (field: keyof PhpSettings, ok: (v: unknown) => boolean) => {
    const v = old[field];
    const key = PHP_SETTING_KEYS[field];
    if (v !== undefined && ok(v) && !hasValue(key)) out[key] = v as SettingValue;
  };
  take("languageLevel", (v) => (LANGUAGE_LEVELS as readonly unknown[]).includes(v));
  take("cliInterpreter", (v) => typeof v === "string" && v.trim() !== "" && v !== defaultPhpSettings.cliInterpreter);
  take("includePaths", (v) => Array.isArray(v) && v.length > 0 && v.every((p) => typeof p === "string"));
  return out;
}

/** Move PHP settings from localStorage into the settings backend once settings are loaded. */
export function migrateLegacyPhpSettings(): void {
  let raw: string | null = null;
  try {
    raw = localStorage.getItem(LEGACY_STORAGE_KEY);
  } catch {
    return;
  }
  if (raw === null) return;
  createRoot((dispose) =>
    createEffect(
      on(
        () => settingsState.loaded,
        (loaded) => {
          if (!loaded) return;
          dispose();
          const hasValue = (k: string) => k in settingsState.user || k in settingsState.project;
          const changes = legacyPhpChanges(raw, hasValue);
          // The old values applied to every project, so they go to the user layer.
          void Promise.all(Object.entries(changes).map(([k, v]) => saveSetting(k, v, "user")))
            .then(() => localStorage.removeItem(LEGACY_STORAGE_KEY))
            .catch((e) => console.warn("PHP settings could not be migrated", e));
        },
      ),
    ),
  );
}
