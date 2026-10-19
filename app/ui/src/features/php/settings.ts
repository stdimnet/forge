/**
 * PHP settings (Settings > Languages & Frameworks > PHP). Kept in a small per-feature store
 * persisted to localStorage until a shared settings service exists; `toLspSettings` maps
 * them to the backend's `PhpLspSettings`.
 */
import { createStore } from "solid-js/store";
import type { PhpLspSettings, ServerChoice } from "./api";

export interface PhpSettings {
  /** PHP language level, e.g. "8.3". */
  languageLevel: string;
  /** CLI interpreter executable (`php`, `/usr/bin/php8.3`, `docker://...`). */
  cliInterpreter: string;
  server: ServerChoice;
  /** Intelephense premium licence key, stored locally only. */
  licenceKey: string;
  includePaths: string[];
}

export const LANGUAGE_LEVELS = ["7.4", "8.0", "8.1", "8.2", "8.3", "8.4"] as const;

const KEY = "forge.php.settings";

export const defaultPhpSettings: PhpSettings = {
  languageLevel: "8.3",
  cliInterpreter: "php",
  server: "intelephense",
  licenceKey: "",
  includePaths: [],
};

function load(): PhpSettings {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) return { ...defaultPhpSettings, ...(JSON.parse(raw) as Partial<PhpSettings>) };
  } catch {
    /* storage unavailable: use defaults */
  }
  return { ...defaultPhpSettings };
}

const [phpSettings, setStore] = createStore<PhpSettings>(load());

export { phpSettings };

export function updatePhpSettings(patch: Partial<PhpSettings>): void {
  setStore(patch);
  try {
    localStorage.setItem(KEY, JSON.stringify(phpSettings));
  } catch {
    /* ignore */
  }
}

export function toLspSettings(s: PhpSettings = phpSettings): PhpLspSettings {
  return {
    server: s.server,
    licenceKey: s.licenceKey.trim() || null,
    phpVersion: s.languageLevel,
    maxFileSize: 5_000_000,
    extraStubs: [],
    includePaths: [...s.includePaths],
  };
}
