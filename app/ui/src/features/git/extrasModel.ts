/** Path helpers of Get from VCS and New Worktree (no IDE state, so they are easy to test). */
import type { Worktree } from "./extrasApi";

/** Join a parent directory and a name with the separator the parent uses. */
export function joinDir(parent: string, name: string): string {
  const sep = parent.includes("\\") && !parent.includes("/") ? "\\" : "/";
  return parent.replace(/[\\/]+$/, "") + sep + name;
}

/** A sibling folder of the main worktree named after the branch, like PHPStorm suggests. */
export function suggestWorktreePath(list: Worktree[], branch: string): string {
  const main = list.find((w) => w.main) ?? list[0];
  if (!main) return "";
  const parts = main.path.replace(/[\\/]+$/, "").split(/[\\/]/);
  const repo = parts.pop() ?? "worktree";
  const parent = main.path.slice(0, main.path.replace(/[\\/]+$/, "").length - repo.length).replace(/[\\/]+$/, "");
  const suffix = branch.replace(/[^A-Za-z0-9._-]+/g, "-").replace(/^-+|-+$/g, "");
  return joinDir(parent || "/", suffix ? `${repo}-${suffix}` : `${repo}-worktree`);
}
