/**
 * Browser-mode LuaForge backend: the Lua files of the open demo project are indexed by the
 * Rust crate `luaforge-lang-lua` compiled to WebAssembly (`crates/forge-wasm/src/lua.rs`),
 * so structure, hover, Go to Declaration, symbol search and run icons are the real ones.
 * `?project=lua` opens the LÖVE demo game.
 */
import { registerMock } from "@/ipc";
import type { LuaIndexStats } from "@/ipc/lua";
import { callWasm, forgeWasm } from "@/wasm";
import { isLuaDemo, luaDemoInfo, luaDemoName } from "./demoLuaProject";
import { setSetting } from "./settingsMock";
import { mockFiles } from "./workspaceMock";

/** Hand the project's Lua files (and `.luarc.json`) to the WebAssembly index. */
async function indexFiles(): Promise<LuaIndexStats> {
  const wasm = await forgeWasm();
  const files: Record<string, string> = {};
  for (const [path, text] of Object.entries(mockFiles)) {
    if (path.endsWith(".lua") || path === ".luarc.json") files[path] = text;
  }
  return callWasm<LuaIndexStats>(wasm.lua_index_files, { files, name: isLuaDemo() ? luaDemoName : "" });
}

let indexed: Promise<LuaIndexStats> | null = null;
const ready = () => (indexed ??= indexFiles());

/** Run a bridge function once the index is loaded. */
async function call<T>(name: "lua_file_structure" | "lua_hover" | "lua_goto" | "lua_run_markers" | "lua_search_symbols", args: unknown): Promise<T> {
  await ready();
  const wasm = await forgeWasm();
  return callWasm<T>(wasm[name], args);
}

if (isLuaDemo()) {
  registerMock("project_detect", () => structuredClone(luaDemoInfo));
  // Like the other demos, announce the framework only with `?announce`.
  setSetting(
    "project.announcedFrameworks",
    new URLSearchParams(location.search).has("announce") ? null : [`~/Projects/${luaDemoName}#love`],
    "user",
  );
}

registerMock("lua_index_workspace", () => {
  indexed = indexFiles();
  return indexed;
});
registerMock("lua_files_changed", () => {
  indexed = indexFiles();
  return undefined;
});
registerMock("lua_search_symbols", ({ query, kind, limit, exact }) => call("lua_search_symbols", { query, kind, limit, exact }));
registerMock("lua_file_structure", ({ path, content }) => call("lua_file_structure", { path, content }));
registerMock("lua_hover", ({ path, offset, content }) => call("lua_hover", { path, offset, content }));
registerMock("lua_goto", ({ path, offset, content }) => call("lua_goto", { path, offset, content }));
registerMock("lua_run_markers", ({ path, content }) => call("lua_run_markers", { path, content }));
// StyLua is a local process; browser mode behaves as if it were not installed.
registerMock("lua_format", () => null);
