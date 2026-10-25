/**
 * Browser-mode project structure: excluded flags of `fs_list_dir`, the excluded files left
 * out of Find in Files and Go to File, and `project_tree_list` over the in-memory demo
 * project, all computed by the same Rust code as the desktop app
 * (`forge_scopes::{structure, view}` as WebAssembly).
 */
import { registerMock, invoke } from "@/ipc";
import type { FileEntry } from "@/ipc/workspace";
import type { FolderMark, StructureConfig, TreeRow, ViewOptions } from "@/ipc/projectStructure";
import { callWasmSync, forgeWasm, type ForgeWasm } from "@/wasm";
import { mockSettingValue } from "./settingsMock";

let wasm: ForgeWasm | null = null;
void forgeWasm().then((w) => (wasm = w));

/** The project structure from the mock settings. */
export function mockStructureConfig(): StructureConfig {
  return {
    folders: (mockSettingValue("projectStructure.folders") ?? []) as unknown as FolderMark[],
    excludePatterns: String(mockSettingValue("projectStructure.excludePatterns") ?? ""),
  };
}

/** Excluded flags for `[path, isDir]` pairs (all false before the module loaded). */
export function excludedFlags(paths: [string, boolean][]): boolean[] {
  if (!wasm || !paths.length) return paths.map(() => false);
  try {
    return callWasmSync<boolean[]>(wasm.project_structure_excluded, { config: mockStructureConfig(), paths });
  } catch {
    return paths.map(() => false);
  }
}

/** The files of `paths` that are not excluded. */
export function withoutExcluded(paths: string[]): string[] {
  const flags = excludedFlags(paths.map((p) => [p, false]));
  return paths.filter((_, i) => !flags[i]);
}

registerMock("project_tree_list", async ({ path, options }) => {
  const w = await forgeWasm();
  const dir = String(path ?? "");
  const opts = options as ViewOptions | undefined;
  const entries = await invoke<FileEntry[]>("fs_list_dir", { path: dir });
  // Compact Middle Packages looks into single child folders.
  const listing: Record<string, FileEntry[]> = {};
  if (opts?.compactMiddlePackages) {
    const visit = async (d: string, depth: number) => {
      const kids = await invoke<FileEntry[]>("fs_list_dir", { path: d });
      listing[d] = kids;
      if (depth < 32 && kids.length === 1 && kids[0].kind === "dir") await visit(kids[0].path, depth + 1);
    };
    for (const e of entries) if (e.kind === "dir") await visit(e.path, 0);
  }
  const view = (list: FileEntry[]) => list.map((e) => ({ ...e, ignored: !!e.ignored, excluded: !!e.excluded }));
  return callWasmSync<TreeRow[]>(w.project_view_arrange, {
    entries: view(entries),
    listing: Object.fromEntries(Object.entries(listing).map(([k, v]) => [k, view(v)])),
    options: opts,
  });
});
