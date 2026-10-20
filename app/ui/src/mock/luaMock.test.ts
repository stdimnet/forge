import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { LuaFileStructure, LuaHover, LuaLocation, LuaRunMarker, LuaSymbolHit } from "@/ipc/lua";
import { hitDetail, isLuaPath, markerConfiguration, toStructureNode } from "@/features/lua/model";
import { luaDemoFiles } from "./demoLuaProject";
import "./luaMock";

const VECTOR = luaDemoFiles["lib/vector.lua"];
const MAIN = `local Vec2 = require("lib.vector")\nlocal v = Vec2.new(1, 2)\nprint(v:len())\n`;

describe("Lua mock (Rust index through WebAssembly)", () => {
  it("indexes files, navigates through require and annotations", async () => {
    const s = await invoke<LuaFileStructure | null>("lua_file_structure", { path: "lib/vector.lua", content: VECTOR });
    expect(s?.nodes[0]).toMatchObject({ name: "Vec2", kind: "class" });
    expect(s?.nodes[0].children?.map((c) => c.name)).toEqual(["x", "y", "__index", "new", "len", "scale", "__add"]);

    const atRequire = MAIN.indexOf("lib.vector") + 2;
    const toModule = await invoke<LuaLocation[]>("lua_goto", { path: "main.lua", offset: atRequire, content: MAIN });
    expect(toModule[0].path).toBe("lib/vector.lua");

    const atLen = MAIN.indexOf("len") + 1;
    const toMethod = await invoke<LuaLocation[]>("lua_goto", { path: "main.lua", offset: atLen, content: MAIN });
    expect(toMethod[0]).toMatchObject({ path: "lib/vector.lua", line: VECTOR.split("\n").findIndex((l) => l.includes("Vec2:len")) });

    const hover = await invoke<LuaHover | null>("lua_hover", { path: "main.lua", offset: atLen, content: MAIN });
    expect(hover?.doc.signature).toBe("function Vec2:len(): number");
    expect(hover?.language).toBe("lua");

    const hits = await invoke<LuaSymbolHit[]>("lua_search_symbols", { query: "vec", kind: "class", limit: 10 });
    expect(hits[0]).toMatchObject({ name: "Vec2", kind: "class", path: "lib/vector.lua" });

    const markers = await invoke<LuaRunMarker[]>("lua_run_markers", { path: "main.lua", content: MAIN });
    expect(markers).toEqual([{ line: 0, kind: "script", name: "main", file: "main.lua" }]);
    expect(await invoke("lua_run_markers", { path: "lib/vector.lua" })).toEqual([]);
  });

  it("maps index answers to UI models", () => {
    expect(isLuaPath("src/a.lua")).toBe(true);
    expect(isLuaPath("x-1.0-1.rockspec")).toBe(true);
    expect(isLuaPath("a.php")).toBe(false);
    expect(toStructureNode({ name: "hp", kind: "field", detail: ": integer", offset: 3, line: 1, column: 2 })).toMatchObject({
      kind: "property",
      column: 2,
    });
    const hit = { name: "M", kind: "module", container: "util", path: "util.lua", offset: 0, line: 0, column: 0, score: 1, positions: [] } as const;
    expect(hitDetail(hit)).toBe('require("util")');
    expect(markerConfiguration({ line: 0, kind: "love", name: "game", file: ".", interpreter: "love" })).toMatchObject({
      type: "lua-script",
      interpreter: "love",
      options: { file: "." },
    });
  });
});
