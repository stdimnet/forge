import { describe, expect, it } from "vitest";
import { providedEntries, registerContextMenuProvider, resolveEntries, type ResolvedItem } from "@/features/contextMenu/state";
import { registerCommand } from "@/state/commands";
import type { StatusEntry } from "./api";
import { targetState, vcsTargets, type VcsTarget } from "./context";
import { gitSubmenu } from "./fileMenu";

const entry = (path: string, index: StatusEntry["index"], worktree: StatusEntry["worktree"]): StatusEntry => ({
  path,
  origPath: null,
  index,
  worktree,
  conflict: false,
});

const entries: StatusEntry[] = [
  entry("app/Models/User.php", "unmodified", "modified"),
  entry("app/Services/Checkout.php", "added", "unmodified"),
  entry("tests/Feature/OrderTest.php", "untracked", "untracked"),
  entry("tests/Unit/CheckoutTest.php", "untracked", "untracked"),
];

function menu(target: VcsTarget, status = entries): ResolvedItem[] {
  const [git] = resolveEntries([gitSubmenu(target, status)]) as ResolvedItem[];
  expect(git).toMatchObject({ kind: "submenu", label: "Git" });
  return git.items!;
}

const byLabel = (items: (ResolvedItem | "separator")[]) =>
  Object.fromEntries(items.filter((i): i is ResolvedItem => i !== "separator").map((i) => [i.label, i]));
const enabled = (items: ResolvedItem[]) => items.filter((i) => (i as unknown) !== "separator" && !i.disabled).map((i) => i.label);

describe("Project view Git submenu", () => {
  it("lists PHPStorm's items, action ids and separators in order", () => {
    const items = menu({ path: "app/Models/User.php", dir: false }) as (ResolvedItem | "separator")[];
    expect(items.map((i) => (i === "separator" ? "-" : `${i.label}=${i.action}`))).toEqual([
      "Commit File…=CheckinFiles",
      "Add=Git.Add",
      "Add to .gitignore=Git.Ignore.File",
      "-",
      "Annotate with Git Blame=Annotate",
      "Show Diff=Compare.SameVersion",
      "Compare with Revision…=Compare.Selected",
      "Compare with Branch…=Git.CompareWithBranch",
      "Show History=Vcs.ShowTabbedFileHistory",
      "-",
      "Rollback…=ChangesView.Rollback",
      "-",
      "Push…=Vcs.Push",
      "Pull…=Git.Pull",
      "Fetch=Git.Fetch",
      "-",
      "Branches…=Git.Branches",
      "New Branch…=Git.CreateNewBranch",
      "New Tag…=Git.Tag",
      "-",
      "Stash Changes…=Git.Stash",
      "Unstash Changes…=Git.Unstash",
      "-",
      "Manage Remotes…=Git.Configure.Remotes",
    ]);
  });

  it("shows PHPStorm's shortcut hints", () => {
    const items = byLabel(menu({ path: "app/Models/User.php", dir: false }));
    expect(items["Commit File…"].shortcut).toMatch(/K$/);
    expect(items["Add"].shortcut).toMatch(/A$/);
    expect(items["Show Diff"].shortcut).toMatch(/D$/);
    expect(items["Rollback…"].shortcut).toMatch(/Z$/);
    expect(items["Push…"].shortcut).toMatch(/K$/);
    expect(items["Push…"].shortcut).not.toBe(items["Commit File…"].shortcut);
  });

  it("enables commit, diff and rollback for a modified file, not Add", () => {
    const items = menu({ path: "app/Models/User.php", dir: false });
    const on = enabled(items);
    expect(on).toEqual(expect.arrayContaining(["Commit File…", "Annotate with Git Blame", "Show Diff", "Compare with Revision…", "Show History", "Rollback…"]));
    expect(on).not.toContain("Add");
    expect(on).not.toContain("Add to .gitignore");
  });

  it("enables Add and Add to .gitignore for an unversioned file only", () => {
    const on = enabled(menu({ path: "tests/Feature/OrderTest.php", dir: false }));
    expect(on).toEqual(expect.arrayContaining(["Commit File…", "Add", "Add to .gitignore"]));
    for (const label of ["Annotate with Git Blame", "Show Diff", "Compare with Revision…", "Compare with Branch…", "Show History", "Rollback…"]) {
      expect(on).not.toContain(label);
    }
  });

  it("greys out change actions for a clean tracked file", () => {
    const on = enabled(menu({ path: "app/Models/Order.php", dir: false }));
    expect(on).not.toContain("Commit File…");
    expect(on).not.toContain("Show Diff");
    expect(on).not.toContain("Rollback…");
    expect(on).toEqual(expect.arrayContaining(["Annotate with Git Blame", "Compare with Branch…", "Show History"]));
  });

  it("acts on all changes below a directory, without single-file actions", () => {
    const items = menu({ path: "app", dir: true });
    expect(items[0].label).toBe("Commit Directory…");
    const on = enabled(items);
    expect(on).toEqual(expect.arrayContaining(["Commit Directory…", "Show Diff", "Show History", "Rollback…"]));
    expect(on).not.toContain("Annotate with Git Blame");
    expect(on).not.toContain("Compare with Revision…");
    expect(on).not.toContain("Add");
    expect(enabled(menu({ path: "tests", dir: true }))).toEqual(expect.arrayContaining(["Add", "Add to .gitignore"]));
  });

  it("disables Stash Changes without tracked changes", () => {
    const only = entries.filter((e) => e.worktree === "untracked");
    expect(enabled(menu({ path: "tests/Feature/OrderTest.php", dir: false }, only))).not.toContain("Stash Changes…");
  });

  it("runs the action with the clicked file as its context", () => {
    let seen: VcsTarget[] = [];
    registerCommand({ id: "Git.Add", title: "Add", run: () => void (seen = vcsTargets()) });
    const add = byLabel(menu({ path: "tests/Feature/OrderTest.php", dir: false }))["Add"];
    add.run!();
    expect(seen).toEqual([{ path: "tests/Feature/OrderTest.php", dir: false }]);
  });
});

describe("targetState", () => {
  it("separates tracked changes from unversioned files", () => {
    const s = targetState([{ path: "", dir: true }], entries);
    expect(s.changed).toEqual(["app/Models/User.php", "app/Services/Checkout.php"]);
    expect(s.unversioned).toHaveLength(2);
    expect(s.allUnversioned).toBe(false);
  });

  it("treats ignored files as neither tracked nor addable to .gitignore", () => {
    const s = targetState([{ path: ".env", dir: false, ignored: true }], entries);
    expect(s.fileTracked).toBe(false);
    expect(s.allUnversioned).toBe(false);
  });
});

describe("context menu providers", () => {
  it("collects entries contributed for a menu id", () => {
    registerContextMenuProvider<string>("test-menu", (name) => [gitSubmenu({ path: name, dir: false }, entries)]);
    expect(providedEntries("test-menu", "a.php")).toHaveLength(1);
    expect(providedEntries("other-menu", "a.php")).toEqual([]);
  });
});
