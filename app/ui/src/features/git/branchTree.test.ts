import { describe, expect, it } from "vitest";
import { callWasm, forgeWasm } from "@/wasm";
import type { Branches, BranchNode } from "./api";
import { flattenBranchTree, matchRange, searchQuery, toggled, ungrouped, type BranchTreeRow, type Expansion } from "./branchTree";

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

  it("finds branches in closed folders and shows the folders above them open", async () => {
    const t = await trees(["main", "feat/ui/tabs", "feat/api", "bugfix/login-tabs", "chore/deps"], ["origin/feat/ui/tabs", "origin/main"]);
    const closed = new Map([["local:dir:feat", false]]);
    expect(outline(flattenBranchTree(t.localTree, { section: "local", expansion: closed, query: "tabs" }))).toEqual(["bugfix v", "  login-tabs", "feat v", "  ui v", "    tabs"]);
    expect(outline(flattenBranchTree(t.remoteTree, { section: "remote", expansion: none, query: "tabs" }))).toEqual(["origin v", "  feat v", "    ui v", "      tabs"]);
    expect(flattenBranchTree(t.localTree, { section: "local", expansion: none, query: "nothing" })).toEqual([]);
  });

  it("shows a folder whose name matches, closed until it is opened", async () => {
    const t = await trees(["main", "bugfix/phpfixforsmarty", "feature/smarty-to-twig/step-1", "feature/smarty-to-twig/step-2", "feature/api"], []);
    // Like PHPStorm: the branch by its name, the folder by its name; the folder stays closed.
    expect(outline(flattenBranchTree(t.localTree, { section: "local", expansion: none, query: "smarty" }))).toEqual(["bugfix v", "  phpfixforsmarty", "feature v", "  smarty-to-twig >"]);
    // Opened, it shows everything in it, also what does not match itself.
    const open = new Map([["local:dir:feature/smarty-to-twig", true]]);
    expect(outline(flattenBranchTree(t.localTree, { section: "local", expansion: open, query: "smarty" }))).toEqual([
      "bugfix v",
      "  phpfixforsmarty",
      "feature v",
      "  smarty-to-twig v",
      "    step-1",
      "    step-2",
    ]);
    // A match inside a matching folder opens it and shows only the match.
    expect(outline(flattenBranchTree(t.localTree, { section: "local", expansion: none, query: "t" }).slice(-3))).toEqual(["  smarty-to-twig v", "    step-1", "    step-2"]);
    expect(outline(flattenBranchTree(t.localTree, { section: "local", expansion: none, query: "step-2" }))).toEqual(["feature v", "  smarty-to-twig v", "    step-2"]);
  });

  it("matches the full name when the search has a slash", async () => {
    const t = await trees(["feat/ui/tabs", "feat/api", "fix/api"], []);
    expect(outline(flattenBranchTree(t.localTree, { section: "local", expansion: none, query: "feat/" }))).toEqual(["feat v", "  ui v", "    tabs", "  api"]);
    expect(outline(flattenBranchTree(t.localTree, { section: "local", expansion: none, query: "feat/a" }))).toEqual(["feat v", "  api"]);
  });

  it("says where the search is found in a name", () => {
    expect(matchRange("phpfixforSmarty", searchQuery(" SMARTY "))).toEqual([9, 15]);
    expect(matchRange("smarty-to-twig", "smarty")).toEqual([0, 6]);
    expect(matchRange("main", "smarty")).toBeNull();
    expect(matchRange("main", "")).toBeNull();
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
