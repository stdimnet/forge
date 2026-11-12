import { describe, expect, it } from "vitest";
import { callWasm, forgeWasm } from "@/wasm";
import type { Branches, BranchNode } from "./api";
import { flattenBranchTree, toggled, ungrouped, type BranchTreeRow, type Expansion } from "./branchTree";

/** The trees as `git_branches` answers them: built by Rust (`forge_vcs::branch_tree`). */
async function trees(local: string[], remote: string[], current = ""): Promise<Pick<Branches, "localTree" | "remoteTree">> {
  return callWasm((await forgeWasm()).vcs_branch_tree, {
    local: local.map((name) => ({ name, current: name === current })),
    remote: remote.map((name) => ({ name })),
    remotes: ["origin"],
  });
}

const outline = (rows: BranchTreeRow[]) =>
  rows.map((r) => `${"  ".repeat(r.depth)}${r.node.label}${r.node.kind === "branch" ? "" : r.expanded ? " v" : " >"}`);
const none: Expansion = new Map();

describe("branch tree rows", () => {
  it("shows folders closed, except the ones of the current branch and the remotes", async () => {
    const t = await trees(["main", "feat/ui/tabs", "feat/api", "bugfix/login"], ["origin/main", "origin/feat/launch-profiles"], "feat/ui/tabs");
    expect(outline(flattenBranchTree(t.localTree, { section: "local", expansion: none }))).toEqual(["bugfix >", "feat v", "  ui v", "    tabs", "  api", "main"]);
    expect(outline(flattenBranchTree(t.remoteTree, { section: "remote", expansion: none }))).toEqual(["origin v", "  feat >", "  main"]);
  });

  it("opens and closes folders, also the ones that start open", async () => {
    const t = await trees(["main", "feat/ui/tabs", "bugfix/login"], [], "feat/ui/tabs");
    let rows = flattenBranchTree(t.localTree, { section: "local", expansion: none });
    let expansion: Expansion = toggled(none, rows.find((r) => r.node.label === "bugfix")!);
    expansion = toggled(expansion, rows.find((r) => r.node.label === "feat")!);
    rows = flattenBranchTree(t.localTree, { section: "local", expansion });
    expect(outline(rows)).toEqual(["bugfix v", "  login", "feat >", "main"]);
    // A folder inside a closed folder keeps its own state.
    expansion = toggled(expansion, rows.find((r) => r.node.label === "feat")!, true);
    expect(outline(flattenBranchTree(t.localTree, { section: "local", expansion }))).toEqual(["bugfix v", "  login", "feat v", "  ui v", "    tabs", "main"]);
  });

  it("finds branches in closed folders and shows only their folders, open", async () => {
    const t = await trees(["main", "feat/ui/tabs", "feat/api", "bugfix/login-tabs", "chore/deps"], ["origin/feat/ui/tabs", "origin/main"]);
    const match = (name: string) => name.includes("tabs");
    const closed = new Map([["local:dir:feat", false]]);
    expect(outline(flattenBranchTree(t.localTree, { section: "local", expansion: closed, match }))).toEqual(["bugfix v", "  login-tabs", "feat v", "  ui v", "    tabs"]);
    expect(outline(flattenBranchTree(t.remoteTree, { section: "remote", expansion: none, match }))).toEqual(["origin v", "  feat v", "    ui v", "      tabs"]);
    // The folder name is part of the branch name, so it finds every branch in the folder.
    expect(outline(flattenBranchTree(t.localTree, { section: "local", expansion: none, match: (n) => n.includes("feat/") }))).toEqual(["feat v", "  ui v", "    tabs", "  api"]);
    expect(flattenBranchTree(t.localTree, { section: "local", expansion: none, match: () => false })).toEqual([]);
  });

  it("keeps a branch and a folder of the same name apart", async () => {
    const t = await trees(["feat", "feat/x"], []);
    const rows = flattenBranchTree(t.localTree, { section: "local", expansion: new Map([["local:dir:feat", true]]) });
    expect(rows.map((r) => r.key)).toEqual(["local:dir:feat", "local:branch:feat/x", "local:branch:feat"]);
    expect(new Set(rows.map((r) => r.key)).size).toBe(rows.length);
  });

  it("starts deeper for a tree below a section", async () => {
    const t = await trees(["feat/x"], [], "feat/x");
    expect(flattenBranchTree(t.localTree, { section: "local", expansion: none, depth: 1 }).map((r) => r.depth)).toEqual([1, 2]);
  });

  it("lists the branches of a remote without folders when grouping is off", async () => {
    const t = await trees([], ["origin/main", "origin/feat/ui/tabs", "origin/feat/api"]);
    const flat: BranchNode[] = ungrouped(t.remoteTree, ["origin/main", "origin/feat/ui/tabs", "origin/feat/api"]);
    expect(flat.map((n) => n.label)).toEqual(["origin"]);
    expect(flat[0].children.map((n) => [n.label, n.path, n.kind])).toEqual([
      ["main", "origin/main", "branch"],
      ["feat/ui/tabs", "origin/feat/ui/tabs", "branch"],
      ["feat/api", "origin/feat/api", "branch"],
    ]);
  });
});
