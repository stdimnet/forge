/**
 * Scopes and file colors in the UI. The scope language, the scope choosers' entries, the
 * Scopes page's preview tree and the file colors are computed by Rust (`forge-scopes`)
 * through WebAssembly, synchronously, in the desktop app and in browser mode. This module
 * only gathers the inputs: the named scopes and colors from the settings and the context
 * (test folders, open, changed and current file).
 */
import { createEffect, createMemo, createRoot, createSignal, on } from "solid-js";
import { forgeWasm, type ForgeWasm } from "@/wasm";
import { getSetting } from "@/state/settings";
import { editor, isCustomTab } from "@/state/editor";
import { workspace } from "@/state/workspace";
import { GUEST_PREFIX } from "@/features/code-with-me/docs";
import { status } from "@/features/git/store";
import {
  scopesApi,
  type FileColor,
  type NamedScope,
  type ScopeContext,
  type ScopeEditOp,
  type ScopeEntry,
  type ScopeFilter,
  type ScopeParseError,
  type ScopeTree,
} from "@/ipc/scopes";

let wasm: ForgeWasm | null = null;
const [ready, setReady] = createSignal(false);
export { ready as scopesReady };

void forgeWasm().then(
  (w) => {
    wasm = w;
    setReady(true);
  },
  (e) => console.warn("scopes not loaded", e),
);

function call<T>(pick: (w: ForgeWasm) => (json: string) => string, input: unknown, fallback: T): T {
  if (!ready() || !wasm) return fallback;
  try {
    const out = JSON.parse(pick(wasm)(JSON.stringify(input))) as T | { error: string };
    if (out && typeof out === "object" && !Array.isArray(out) && "error" in out && Object.keys(out).length === 1) {
      console.warn("scopes:", (out as { error: string }).error);
      return fallback;
    }
    return out as T;
  } catch (e) {
    console.warn("scopes:", e);
    return fallback;
  }
}

export const SHARED_SCOPES = "scopes.shared";
export const LOCAL_SCOPES = "scopes.local";
export const SHARED_COLORS = "fileColors.shared";
export const LOCAL_COLORS = "fileColors.local";

/** The palette the themes provide (`--file-color-<name>` in `theme.css`). */
export const PALETTE = ["Blue", "Green", "Orange", "Rose", "Violet", "Yellow", "Gray"] as const;

/** CSS value of a palette color. */
export const fileColorVar = (color: string) => `var(--file-color-${color.toLowerCase()})`;

const SWATCHES: Record<string, string> = {
  blue: "var(--graph-1)",
  green: "var(--graph-2)",
  orange: "var(--graph-0)",
  rose: "var(--graph-6)",
  violet: "var(--graph-3)",
  yellow: "var(--graph-4)",
  gray: "var(--fg-muted)",
};

/** The full-strength color of a palette entry, for swatches. */
export const swatchVar = (color: string) => SWATCHES[color.toLowerCase()] ?? "transparent";

export interface ScopeSet {
  shared: NamedScope[];
  local: NamedScope[];
}

/** The named scopes of the settings. */
export const scopeSet = (): ScopeSet => ({
  shared: getSetting<NamedScope[]>(SHARED_SCOPES, []) ?? [],
  local: getSetting<NamedScope[]>(LOCAL_SCOPES, []) ?? [],
});

// ---- context ----

const [testRoots, setTestRoots] = createSignal<string[]>([]);

createRoot(() =>
  createEffect(
    on(
      () => workspace()?.root,
      (root) => {
        setTestRoots([]);
        if (root) void scopesApi.testRoots().then(setTestRoots, () => undefined);
      },
    ),
  ),
);

const projectPath = (p: string) => !p.includes("://") && !isCustomTab(p);

/** What scopes are evaluated against right now. */
export function scopeContext(): ScopeContext {
  return {
    testRoots: testRoots(),
    openFiles: editor.tabs.map((t) => t.path).filter(projectPath),
    changedFiles: [...new Set((status()?.entries ?? []).map((e) => e.path))],
    currentFile: editor.active && projectPath(editor.active) ? editor.active : "",
  };
}

// ---- scopes ----

/** Predefined and named scopes for a chooser, and problems of the named ones. */
export function scopeEntries(set: ScopeSet = scopeSet()): { entries: ScopeEntry[]; problems: [string, string][] } {
  return call((w) => w.scopes_entries, set, { entries: [], problems: [] });
}

/** The filter a search sends for the scope called `name`; `undefined` for an unknown name. */
export function scopeFilter(name: string, context: ScopeContext = scopeContext()): ScopeFilter | undefined {
  const entry = scopeEntries().entries.find((e) => e.name === name);
  if (!entry) return undefined;
  const scope = entry.predefined ? { predefined: entry.predefined } : { pattern: entry.pattern ?? "" };
  return { scope, context };
}

/** Why a pattern does not parse, or null. */
export const checkPattern = (pattern: string): ScopeParseError | null => call((w) => w.scopes_check, { pattern }, null);

/** The pattern after an Include / Exclude button press. */
export const editPattern = (pattern: string, op: ScopeEditOp, path: string, dir: boolean): string =>
  call((w) => w.scopes_edit, { pattern, op, path, dir }, pattern);

/** The Scopes page's preview of a pattern over the project's files. */
export const scopeTree = (files: string[], filter: ScopeFilter): ScopeTree =>
  call((w) => w.scopes_tree, { files, filter }, { nodes: [], matched: 0, total: files.length });

/** Which of `paths` a scope contains. */
export const scopeContains = (filter: ScopeFilter, paths: string[]): boolean[] =>
  call((w) => w.scopes_contains, { filter, paths }, paths.map(() => true));

// ---- file colors ----

interface ColorConfig {
  scopes: ScopeSet;
  local: FileColor[];
  shared: FileColor[];
  context: ScopeContext;
}

const colors = createRoot(() => {
  const config = createMemo<ColorConfig | null>(() => {
    if (!ready() || getSetting<boolean>("fileColors.enabled", true) === false) return null;
    return {
      scopes: scopeSet(),
      local: getSetting<FileColor[]>(LOCAL_COLORS, []) ?? [],
      shared: getSetting<FileColor[]>(SHARED_COLORS, []) ?? [],
      context: { ...scopeContext(), openFiles: [], currentFile: "" },
    };
  });
  // A new cache whenever the colors, scopes or context change.
  const cache = createMemo(() => (config(), new Map<string, string | null>()));
  return { config, cache };
});

/** The palette color of a file or folder (`undefined` without one). Reactive. */
export function fileColor(path: string, dir = false): string | undefined {
  const config = colors.config();
  if (!config) return undefined;
  const cache = colors.cache();
  const key = `${dir ? "d" : "f"}:${path}`;
  let color = cache.get(key);
  if (color === undefined) {
    color = call<(string | null)[]>((w) => w.file_colors_resolve, { ...config, targets: [{ path, dir }] }, [null])[0] ?? null;
    cache.set(key, color);
  }
  return color ?? undefined;
}

/** Features that colour their own tabs (e.g. database consoles of a production data source). */
const tabColorProviders: ((path: string) => string | undefined)[] = [];

/** Add a provider of editor tab backgrounds (a CSS colour); it wins over file colours. */
export function registerTabColorProvider(provider: (path: string) => string | undefined): void {
  tabColorProviders.push(provider);
}

/** The background of an editor tab (with "Use in editor tabs"). */
export const tabColor = (path: string): string | undefined => {
  for (const p of tabColorProviders) {
    const c = p(path);
    if (c) return c;
  }
  // A Code With Me guest's tab of the host's file (`cwm://<path>`) is coloured like the host's tab.
  const projectFile = path.startsWith(GUEST_PREFIX) ? path.slice(GUEST_PREFIX.length) : path;
  return getSetting<boolean>("fileColors.useInEditorTabs", true) !== false ? colorValue(fileColor(projectFile)) : undefined;
};

/** The background of a Project view row (with "Use in project view"). */
export const projectViewColor = (path: string, dir: boolean): string | undefined =>
  getSetting<boolean>("fileColors.useInProjectView", true) !== false ? colorValue(fileColor(path, dir)) : undefined;

const colorValue = (c: string | undefined) => (c ? fileColorVar(c) : undefined);
