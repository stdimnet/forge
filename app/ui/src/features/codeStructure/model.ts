/**
 * The code structure of open documents, computed in Rust (`forge_codeintel::structure`,
 * `forge_markdown::doc_comments`) and called synchronously through WebAssembly in the desktop
 * app and in browser mode alike: Sticky Lines and breadcrumbs follow every scroll and caret
 * move and folding asks per line, which an IPC round trip could not keep up with. Answers
 * are cached per document version (CodeMirror's immutable `Text`).
 */
import { createSignal } from "solid-js";
import type { Text } from "@codemirror/state";
import type { HoverDoc } from "@/ipc/markdown";
import { getSetting } from "@/state/settings";
import { forgeWasm, type ForgeWasm } from "@/wasm";

export type ScopeKind =
  | "namespace"
  | "class"
  | "interface"
  | "trait"
  | "enum"
  | "function"
  | "method"
  | "closure"
  | "control"
  | "object"
  | "rule"
  | "tag";

/** A scope of a document; lines are 0-based, columns UTF-16. */
export interface Scope {
  kind: ScopeKind;
  name: string;
  /** First line of the header (the sticky line). */
  line: number;
  /** Where navigation puts the caret. */
  column: number;
  startColumn: number;
  openLine: number;
  endLine: number;
  endColumn: number;
  parent?: number;
  sticky: boolean;
  crumb: boolean;
}

/** A custom folding region (`// region Name` … `// endregion`). */
export interface Region {
  name: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  collapsed: boolean;
}

export interface EditorStructure {
  scopes: Scope[];
  regions: Region[];
}

/** A PHPDoc / JSDoc / LuaDoc comment and its rendered parts. */
export interface DocCommentInfo {
  line: number;
  endLine: number;
  column: number;
  doc: HoverDoc;
}

export interface StructureLanguage {
  id: string;
  label: string;
}

const EMPTY: EditorStructure = { scopes: [], regions: [] };

let wasm: ForgeWasm | null = null;
const [structureReady, setStructureReady] = createSignal(false);

/** Whether the Rust side is loaded (reactive); until then documents have no structure. */
export { structureReady };

/** Resolves once the module is loaded; loading starts when the editor is imported. */
export const structureLoaded: Promise<boolean> = forgeWasm().then(
  (w) => {
    wasm = w;
    setStructureReady(true);
    return true;
  },
  (e) => {
    console.warn("editor structure not loaded", e);
    return false;
  },
);

function call<T>(fn: (json: string) => string, input: unknown, fallback: T): T {
  try {
    const out = JSON.parse(fn(JSON.stringify(input))) as T | { error: string };
    if (out && typeof out === "object" && !Array.isArray(out) && "error" in out && Object.keys(out).length === 1) {
      console.warn("editor structure:", (out as { error: string }).error);
      return fallback;
    }
    return out as T;
  } catch (e) {
    console.warn("editor structure:", e);
    return fallback;
  }
}

/** Per document version and language. */
const structures = new WeakMap<Text, Map<string, EditorStructure>>();
const docComments = new WeakMap<Text, Map<string, DocCommentInfo[]>>();

function cached<T>(map: WeakMap<Text, Map<string, T>>, doc: Text, language: string, compute: () => T | null): T | null {
  let byLang = map.get(doc);
  const hit = byLang?.get(language);
  if (hit) return hit;
  const value = compute();
  if (value === null) return null;
  if (!byLang) map.set(doc, (byLang = new Map()));
  byLang.set(language, value);
  return value;
}

/** Scopes and regions of a document (empty until the module is loaded). */
export function structureOf(doc: Text, language: string | undefined): EditorStructure {
  if (!language) return EMPTY;
  return (
    cached(structures, doc, language, () =>
      wasm ? call<EditorStructure>(wasm.editor_structure, { text: doc.toString(), language }, EMPTY) : null,
    ) ?? EMPTY
  );
}

let lastText: { text: string; language: string; value: EditorStructure } | null = null;

/** Scopes and regions of a text that is not in an editor (the Structure view). */
export function structureOfText(text: string, language: string | undefined): EditorStructure {
  if (!language || !wasm) return EMPTY;
  if (lastText && lastText.text === text && lastText.language === language) return lastText.value;
  const value = call<EditorStructure>(wasm.editor_structure, { text, language }, EMPTY);
  lastText = { text, language, value };
  return value;
}

/** The documentation comments of a document. */
export function docCommentsOf(doc: Text, language: string | undefined): DocCommentInfo[] {
  if (!language) return [];
  return (
    cached(docComments, doc, language, () =>
      wasm ? call<DocCommentInfo[]>(wasm.editor_doc_comments, { text: doc.toString(), language }, []) : null,
    ) ?? []
  );
}

/** Structure view nodes with region nodes (`kind: "region"`) around the members inside. */
export function groupByRegions<T extends { line: number; children?: T[] }>(nodes: T[], regions: Region[]): T[] {
  if (!regions.length || !wasm) return nodes;
  return call<T[]>(wasm.structure_group_by_regions, { nodes, regions }, nodes);
}

let languages: StructureLanguage[] | null = null;

/** The languages with Sticky Lines and breadcrumbs. */
export function structureLanguages(): StructureLanguage[] {
  if (languages) return languages;
  if (!wasm) return [];
  languages = call<StructureLanguage[]>(wasm.editor_structure_languages, undefined, []);
  return languages;
}

export const hasStructure = (language: string | undefined): boolean =>
  !!language && structureLanguages().some((l) => l.id === language);

export const languageLabelOf = (language: string): string =>
  structureLanguages().find((l) => l.id === language)?.label ?? language;

/** The scopes around a position, outermost first. */
export function scopesAt(structure: EditorStructure, line: number, column: number): Scope[] {
  return structure.scopes.filter(
    (s) =>
      (s.line < line || (s.line === line && s.startColumn <= column)) &&
      (s.endLine > line || (s.endLine === line && s.endColumn >= column)),
  );
}

// ---- Settings ----

export const STICKY_DISABLED = "editor.stickyLines.disabledLanguages";
export const CRUMBS_DISABLED = "editor.breadcrumbs.disabledLanguages";

const enabledFor = (key: string, language: string | undefined) =>
  structureReady() && !!language && hasStructure(language) && !getSetting<string[]>(key, []).includes(language);

/** Sticky Lines are on for the language (reactive). */
export const stickyLinesOn = (language: string | undefined): boolean =>
  getSetting<boolean>("editor.stickyLines.enabled", true) && enabledFor(STICKY_DISABLED, language);

/** Breadcrumbs are on for the language (reactive). */
export const breadcrumbsOn = (language: string | undefined): boolean =>
  getSetting<boolean>("editor.breadcrumbs.show", true) && enabledFor(CRUMBS_DISABLED, language);

export const breadcrumbsOnTop = (): boolean => getSetting<string>("editor.breadcrumbs.placement", "bottom") === "top";

export const maxStickyLines = (): number => Math.max(1, getSetting<number>("editor.stickyLines.maxLines", 5));
