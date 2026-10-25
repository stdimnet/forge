/**
 * Project structure in the UI: the stored excluded / resource folders and "Exclude files"
 * patterns (project settings), the Project view options (app settings) and the Rust rules
 * (`forge_scopes::{structure, view}`) called synchronously through WebAssembly in both modes.
 */
import { createSignal } from "solid-js";
import { forgeWasm, type ForgeWasm } from "@/wasm";
import { getSetting, saveSetting } from "@/state/settings";
import type { SettingValue } from "@/ipc/settings";
import type {
  FolderMark,
  FolderState,
  MarkAction,
  NestingRule,
  StructureConfig,
  ViewOptions,
} from "@/ipc/projectStructure";

export const FOLDERS_KEY = "projectStructure.folders";
export const PATTERNS_KEY = "projectStructure.excludePatterns";

export const VIEW_KEYS = {
  showExcluded: "projectView.showExcludedFiles",
  showMembers: "projectView.showMembers",
  compact: "projectView.compactMiddlePackages",
  foldersOnTop: "projectView.foldersAlwaysOnTop",
  sortByType: "projectView.sortByType",
  nesting: "projectView.fileNesting.enabled",
  nestingRules: "projectView.fileNesting.rules",
} as const;

let wasm: ForgeWasm | null = null;
const [ready, setReady] = createSignal(false);
export { ready as structureReady };

void forgeWasm().then(
  (w) => {
    wasm = w;
    setReady(true);
  },
  (e) => console.warn("project structure not loaded", e),
);

function call<T>(pick: (w: ForgeWasm) => (json: string) => string, input: unknown, fallback: T): T {
  if (!ready() || !wasm) return fallback;
  try {
    const out = JSON.parse(pick(wasm)(JSON.stringify(input))) as T | { error: string };
    if (out && typeof out === "object" && !Array.isArray(out) && "error" in out && Object.keys(out).length === 1) {
      console.warn("project structure:", (out as { error: string }).error);
      return fallback;
    }
    return out as T;
  } catch (e) {
    console.warn("project structure:", e);
    return fallback;
  }
}

/** The stored project structure (reactive). */
export function structureConfig(): StructureConfig {
  return {
    folders: (getSetting<SettingValue>(FOLDERS_KEY, []) ?? []) as unknown as FolderMark[],
    excludePatterns: String(getSetting<string>(PATTERNS_KEY, "") ?? ""),
  };
}

/** How the entry at `path` is marked (reactive). */
export function folderState(path: string, isDir: boolean, config = structureConfig()): FolderState {
  return call((w) => w.project_structure_state, { config, path, isDir }, { kind: null, reason: null });
}

/** The folders after a "Mark Directory as" action. */
export function markedFolders(path: string, action: MarkAction, config = structureConfig()): FolderMark[] {
  return call((w) => w.project_structure_mark, { config, path, action }, config.folders);
}

/** Apply a "Mark Directory as" action to the project settings. */
export async function markDirectory(path: string, action: MarkAction): Promise<void> {
  await saveSetting(FOLDERS_KEY, markedFolders(path, action) as unknown as SettingValue, "project");
}

/** The folders of Settings | Directories, sorted by kind and path. */
export function describeFolders(config: StructureConfig): FolderMark[] {
  return call((w) => w.project_structure_describe, { config }, config.folders);
}

/** PHPStorm's predefined File Nesting rules. */
export function defaultNestingRules(): NestingRule[] {
  if (!ready() || !wasm) return [];
  try {
    return JSON.parse(wasm.project_view_nesting_defaults()) as NestingRule[];
  } catch {
    return [];
  }
}

/** The stored File Nesting rules; empty means the predefined ones. */
export const storedNestingRules = (): NestingRule[] =>
  (getSetting<SettingValue>(VIEW_KEYS.nestingRules, []) ?? []) as unknown as NestingRule[];

/** The Project view options from the settings (reactive). */
export function viewOptions(): ViewOptions {
  return {
    showExcluded: getSetting<boolean>(VIEW_KEYS.showExcluded, true),
    foldersOnTop: getSetting<boolean>(VIEW_KEYS.foldersOnTop, true),
    sortByType: getSetting<boolean>(VIEW_KEYS.sortByType, false),
    compactMiddlePackages: getSetting<boolean>(VIEW_KEYS.compact, false),
    fileNesting: getSetting<boolean>(VIEW_KEYS.nesting, true),
    nestingRules: storedNestingRules(),
  };
}

export const showMembers = () => getSetting<boolean>(VIEW_KEYS.showMembers, false);
