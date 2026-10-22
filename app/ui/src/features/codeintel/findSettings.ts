/**
 * The Find Usages settings (Ctrl+Shift+Alt+F7, the "Settings" buttons of the Find tool
 * window and the Show Usages popup): the options and the scope are settings
 * (`findUsages.options.*`, `findUsages.scope.*`, `findUsages.openInNewTab`), so every
 * later Find Usages run uses them. Which options an element has and what they do is
 * decided in Rust (`forge_codeintel::usage_options`); the UI forwards the settings and
 * what only it knows: the current, open and recently changed files.
 */
import { createSignal } from "solid-js";
import type { EditorView } from "@codemirror/view";
import { editor, isDirty } from "@/state/editor";
import { getSetting, settingsState } from "@/state/settings";
import { status as gitStatus } from "@/features/git/store";
import { codeApi, isLibraryPath, type SearchScope, type UsageOptions, type UsageScope, type UsageTarget } from "@/ipc/codeintel";
import { showHint } from "@/features/lsp/hint";
import { toLsp } from "@/features/lsp/util";
import type { LspPosition } from "@/ipc/lsp";
import { scopeContext, scopeEntries } from "@/features/scopes/model";

export const OPTIONS_PREFIX = "findUsages.options.";

/** The Find Usages options from the settings, by field (`classMethodUsages`). */
export function usageOptions(): UsageOptions {
  const out: UsageOptions = {};
  for (const spec of settingsState.schema) {
    if (spec.key.startsWith(OPTIONS_PREFIX)) out[spec.key.slice(OPTIONS_PREFIX.length)] = getSetting<boolean>(spec.key);
  }
  return out;
}

export const SCOPES: { value: SearchScope; label: string }[] = [
  { value: "projectFiles", label: "Project Files" },
  { value: "projectAndLibraries", label: "Project and Libraries" },
  { value: "openFiles", label: "Open Files" },
  { value: "currentFile", label: "Current File" },
  { value: "recentlyChangedFiles", label: "Recently Changed Files" },
  { value: "directory", label: "Directory" },
  { value: "custom", label: "Custom" },
];

/** Files changed recently: the working tree's changes and unsaved editors. */
function changedFiles(): string[] {
  const out = new Set((gitStatus()?.entries ?? []).map((e) => e.path));
  for (const t of editor.tabs) if (isDirty(t)) out.add(t.path);
  return [...out];
}

/** Patterns of predefined scopes that Find Usages has no scope kind for. */
const PREDEFINED_PATTERNS: Record<string, string> = {
  projectProductionFiles: "src:*..*",
  projectTestFiles: "test:*..*",
};

/**
 * The custom scope's pattern: a named scope of the Scopes page (`findUsages.scope.name`),
 * else the typed pattern (a scope pattern or globs, decided in Rust).
 */
export function customScopePattern(name: string, pattern: string): { pattern: string; scopeName?: string } {
  if (!name) return { pattern };
  const entry = scopeEntries().entries.find((e) => e.name === name);
  const named = entry?.pattern ?? (entry?.predefined ? PREDEFINED_PATTERNS[entry.predefined] : undefined);
  return named !== undefined ? { pattern: named, scopeName: name } : { pattern };
}

/** Scopes the Find Usages dialog offers besides its own kinds: named and test/production scopes. */
export const extraUsageScopes = () =>
  scopeEntries().entries.filter((e) => !e.predefined || PREDEFINED_PATTERNS[e.predefined] !== undefined);

/** The scope from the settings (or `kind`), with the workbench's files. */
export function usageScope(currentFile: string, kind?: SearchScope): UsageScope {
  const custom = customScopePattern(getSetting<string>("findUsages.scope.name", ""), getSetting<string>("findUsages.scope.pattern", ""));
  return {
    kind: kind ?? getSetting<SearchScope>("findUsages.scope.kind", "projectFiles"),
    directory: getSetting<string>("findUsages.scope.directory", ""),
    includeSubdirectories: getSetting<boolean>("findUsages.scope.includeSubdirectories", true),
    pattern: custom.pattern,
    scopeName: custom.scopeName,
    testRoots: scopeContext().testRoots,
    currentFile,
    openFiles: editor.tabs.map((t) => t.path).filter((p) => !isLibraryPath(p)),
    changedFiles: changedFiles(),
  };
}

// ---- The Find Usages dialog ----

export interface FindUsagesDialogRequest {
  target: UsageTarget;
  /** The directory of the current file, the Directory scope's suggestion. */
  directory: string;
  /** Run the search with the saved settings. */
  find: () => void;
}

const [findUsagesDialog, setFindUsagesDialog] = createSignal<FindUsagesDialogRequest | null>(null);
export { findUsagesDialog };
export const closeFindUsagesDialog = () => setFindUsagesDialog(null);

const NO_ELEMENT = "Cannot search for usages. Position to an element to find usages of, and try again.";

/**
 * Open the Find Usages dialog for the element at an LSP position of `path`; `find` runs
 * after the choices are saved ("Find"). `content` is the editor's text.
 */
export async function openUsageSettingsAt(
  path: string,
  pos: LspPosition,
  content: string | undefined,
  find: () => void,
  hint: (text: string) => void = () => {},
): Promise<void> {
  const target = await codeApi.usageTarget(path, pos, content).catch(() => null);
  if (!target) {
    hint(NO_ELEMENT);
    return;
  }
  setFindUsagesDialog({ target, directory: path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "", find });
}

/** The Find Usages dialog for the element at the caret position `pos` of an editor. */
export function openFindUsagesSettings(path: string, view: EditorView, pos: number, find: () => void): Promise<void> {
  return openUsageSettingsAt(path, toLsp(view.state.doc, pos), view.state.doc.toString(), find, (text) => showHint(view, pos, text));
}
