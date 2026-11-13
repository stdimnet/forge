/**
 * LuaForge IPC (app/src-tauri/src/commands/lua.rs, crate packs/luaforge/lang-lua). Offsets,
 * lines and columns are UTF-16 / 0-based; browser mode answers through WebAssembly.
 */
import { invoke } from "./index";
import { exactMatching } from "./search";
import type { HoverDoc } from "./markdown";

export type LuaSymbolKind = "class" | "alias" | "module" | "function" | "method" | "field" | "variable";

export interface LuaIndexStats {
  files: number;
  functions: number;
  classes: number;
}

export interface LuaSymbolHit {
  name: string;
  kind: LuaSymbolKind;
  /** Owner (`Player`, `M`) or the module name of a module table. */
  container: string;
  path: string;
  offset: number;
  line: number;
  column: number;
  score: number;
  positions: number[];
  deprecated?: boolean;
}

export interface LuaStructureNode {
  name: string;
  kind: LuaSymbolKind;
  detail: string;
  offset: number;
  line: number;
  column: number;
  deprecated?: boolean;
  children?: LuaStructureNode[];
}

export interface LuaFileStructure {
  path: string;
  nodes: LuaStructureNode[];
}

export interface LuaHover {
  doc: HoverDoc;
  start: number;
  end: number;
  /** Language of `doc.signature`. */
  language: string;
}

export interface LuaLocation {
  path: string;
  offset: number;
  line: number;
  column: number;
  /** The target's line, for the "Choose Declaration" popup. */
  preview?: string;
}

export interface LuaRunMarker {
  line: number;
  kind: "script" | "love";
  /** Configuration name. */
  name: string;
  /** File (or LÖVE game directory) passed to the interpreter. */
  file: string;
  /** `love` for LÖVE games; otherwise the configured Lua interpreter is used. */
  interpreter?: string;
}

export const luaApi = {
  indexWorkspace: () => invoke<LuaIndexStats>("lua_index_workspace"),
  filesChanged: (changed: string[], removed: string[]) => invoke<void>("lua_files_changed", { changed, removed }),
  searchSymbols: (query: string, kind: "class" | "symbol" = "class", limit = 100) =>
    invoke<LuaSymbolHit[]>("lua_search_symbols", { query, kind, limit, exact: exactMatching() }),
  fileStructure: (path: string, content?: string) => invoke<LuaFileStructure | null>("lua_file_structure", { path, content }),
  hover: (path: string, offset: number, content?: string) => invoke<LuaHover | null>("lua_hover", { path, offset, content }),
  goto: (path: string, offset: number, content?: string) => invoke<LuaLocation[]>("lua_goto", { path, offset, content }),
  runMarkers: (path: string, content?: string) => invoke<LuaRunMarker[]>("lua_run_markers", { path, content }),
  /** Formatted text, or null when StyLua is off or not installed. */
  format: (path: string, content: string) => invoke<string | null>("lua_format", { path, content }),
};
