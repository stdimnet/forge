/**
 * Pure mappings between the Lua index answers (Rust, `luaforge-lang-lua`) and the shared UI
 * models: Structure tool window nodes, Search Everywhere rows and run configurations.
 */
import type { LuaRunMarker, LuaStructureNode, LuaSymbolHit, LuaSymbolKind } from "@/ipc/lua";
import type { RunConfiguration } from "@/ipc/run";
import type { StructureNode, SymbolKind } from "@/features/php/api";
import type { SearchItemKind } from "@/features/search/providers";

export const isLuaPath = (path: string) => /\.(lua|rockspec)$/i.test(path) || /(^|\/)\.luacheckrc$/.test(path);

/** Lua kinds as the badges of the Structure tool window (fields look like properties). */
export const STRUCTURE_KIND: Record<LuaSymbolKind, SymbolKind> = {
  class: "class",
  alias: "alias",
  module: "module",
  function: "function",
  method: "method",
  field: "property",
  variable: "variable",
};

export const SEARCH_KIND: Record<LuaSymbolKind, SearchItemKind> = {
  class: "class",
  alias: "interface",
  module: "class",
  function: "function",
  method: "method",
  field: "property",
  variable: "constant",
};

export function toStructureNode(n: LuaStructureNode): StructureNode {
  return {
    name: n.name,
    kind: STRUCTURE_KIND[n.kind],
    detail: n.detail,
    modifiers: {},
    offset: n.offset,
    line: n.line,
    column: n.column,
    deprecated: n.deprecated,
    children: n.children?.map(toStructureNode),
  };
}

/** Search row texts: members show their owner, modules their module name. */
export function hitDetail(hit: LuaSymbolHit): string | undefined {
  if (hit.kind === "module") return hit.container ? `require("${hit.container}")` : undefined;
  return hit.container || undefined;
}

/** The (temporary) run configuration a gutter run icon starts. */
export function markerConfiguration(marker: LuaRunMarker): RunConfiguration {
  return {
    name: marker.name,
    type: "lua-script",
    interpreter: marker.interpreter ?? "",
    interpreterOptions: "",
    arguments: "",
    workingDirectory: "",
    env: {},
    options: { file: marker.file },
  };
}
