/**
 * PHP settings (Settings > Languages & Frameworks > PHP). Kept in a small per-feature store
 * persisted to localStorage until a shared settings service exists; `toLspSettings` maps
 * them to the backend's `PhpLspSettings`. The Intelephense licence key is not one of them:
 * it lives in the OS keychain (see `features/langservers/Licence.tsx`).
 */
import { createStore } from "solid-js/store";
import { langserversApi } from "@/ipc/langservers";
import type { PhpLspSettings, ServerChoice } from "./api";

export interface PhpSettings {
  /** PHP language level, e.g. "8.3". */
  languageLevel: string;
  /** CLI interpreter executable (`php`, `/usr/bin/php8.3`, `docker://...`). */
  cliInterpreter: string;
  server: ServerChoice;
  includePaths: string[];
}

export const LANGUAGE_LEVELS = ["7.4", "8.0", "8.1", "8.2", "8.3", "8.4"] as const;

const KEY = "forge.php.settings";

export const defaultPhpSettings: PhpSettings = {
  languageLevel: "8.3",
  cliInterpreter: "php",
  server: "intelephense",
  includePaths: [],
};

/**
 * Older versions kept the licence key here in plain text: move it to the keychain and
 * drop it from storage once that worked.
 */
export function migrateLicenceKey(stored: Record<string, unknown>, save: (key: string) => Promise<unknown>): Promise<boolean> {
  const key = typeof stored.licenceKey === "string" ? stored.licenceKey.trim() : "";
  if (!("licenceKey" in stored)) return Promise.resolve(false);
  const done = () => {
    delete stored.licenceKey;
    return true;
  };
  if (!key) return Promise.resolve(done());
  return save(key).then(done, () => false);
}

function load(): PhpSettings {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) {
      const stored = JSON.parse(raw) as Record<string, unknown>;
      void migrateLicenceKey(stored, (k) => langserversApi.setLicence(k)).then((migrated) => {
        if (migrated) localStorage.setItem(KEY, JSON.stringify(stored));
      });
      const { licenceKey: _plain, ...rest } = stored;
      return { ...defaultPhpSettings, ...(rest as Partial<PhpSettings>) };
    }
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
    // Filled in by the backend from the keychain.
    licenceKey: null,
    phpVersion: s.languageLevel,
    maxFileSize: 5_000_000,
    extraStubs: [],
    includePaths: [...s.includePaths],
  };
}
