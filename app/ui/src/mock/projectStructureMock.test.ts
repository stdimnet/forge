import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { FileEntry } from "@/ipc/workspace";
import type { TreeRow, ViewOptions } from "@/ipc/projectStructure";
import { forgeWasm } from "@/wasm";
import "./settingsMock";
import "./workspaceMock";
import "./findMock";
import "./projectStructureMock";

const options: ViewOptions = {
  showExcluded: true,
  foldersOnTop: true,
  sortByType: false,
  compactMiddlePackages: false,
  fileNesting: true,
  nestingRules: [],
};

describe("project structure mock", () => {
  it("nests files and flags excluded folders like the desktop app", async () => {
    await forgeWasm();
    const rows = await invoke<TreeRow[]>("project_tree_list", { path: "", options });
    const composer = rows.find((r) => r.path === "composer.json");
    expect(composer?.nested?.map((n) => n.path)).toEqual(["composer.lock"]);
    expect(rows.some((r) => r.path === "composer.lock")).toBe(false);

    await invoke("settings_set", {
      key: "projectStructure.folders",
      value: [{ path: "storage/logs", kind: "excluded" }],
      scope: "project",
    });
    const storage = await invoke<FileEntry[]>("fs_list_dir", { path: "storage" });
    expect(storage.find((e) => e.path === "storage/logs")?.excluded).toBe(true);
    const hidden = await invoke<TreeRow[]>("project_tree_list", { path: "storage", options: { ...options, showExcluded: false } });
    expect(hidden.map((r) => r.path)).not.toContain("storage/logs");
    await invoke("settings_set", { key: "projectStructure.folders", value: null, scope: "project" });
  });
});
