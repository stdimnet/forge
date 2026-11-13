/**
 * What file-level Git actions act on (PHPStorm's data context): the Project view selection,
 * the Commit window selection or the active editor. Menus pass their target explicitly with
 * `withVcsTargets`; shortcuts use whatever has the focus.
 */
import { activeTab } from "@/state/editor";
import { projectPathOf } from "@/ipc/projectPath";
import type { StatusEntry } from "./api";

/** A file or folder, workspace-relative; the project root is the folder `""`. */
export interface VcsTarget {
  path: string;
  dir: boolean;
  /** Matched by `.gitignore` (and not tracked). */
  ignored?: boolean;
}

let override: VcsTarget[] | null = null;
let commitSelection: () => string | null = () => null;
let commitFiles: () => string[] = () => [];

/** Lets the Commit window report its selected file and the files listed in it. */
export function setCommitSelectionSource(selected: () => string | null, files: () => string[]): void {
  commitSelection = selected;
  commitFiles = files;
}

/** Files the diff viewer can step through: the Commit window's list when it has the focus. */
export function diffSiblings(fallback: string[]): string[] {
  const focus = typeof document === "undefined" ? null : document.activeElement;
  return !override && focus?.closest(".commit-panel") ? commitFiles() : fallback;
}

/**
 * Run `fn` with `targets` as the action context. Commands read their targets synchronously
 * when they start, so this also covers async commands.
 */
export function withVcsTargets<T>(targets: VcsTarget[], fn: () => T): T {
  const prev = override;
  override = targets;
  try {
    return fn();
  } finally {
    override = prev;
  }
}

export function vcsTargets(): VcsTarget[] {
  if (override) return override;
  const focus = typeof document === "undefined" ? null : document.activeElement;
  const tree = focus?.closest(".project-tree");
  if (tree) {
    const row = tree.querySelector<HTMLElement>(".tree-row.selected[data-path]");
    return row ? [{ path: row.dataset.path!, dir: row.dataset.dir !== undefined, ignored: row.classList.contains("ignored") }] : [];
  }
  if (focus?.closest(".commit-panel")) {
    const path = commitSelection();
    return path ? [{ path, dir: false }] : [];
  }
  // Other tool windows have no file context.
  if (focus?.closest(".tool-window")) return [];
  const tab = activeTab();
  return tab && !tab.view ? [{ path: tab.path, dir: false }] : [];
}

// The status names project files; a Code With Me guest's `cwm://` tab is its project file.
const covers = (t: VcsTarget, path: string) => {
  const target = projectPathOf(t.path);
  return t.dir ? target === "" || path === target || path.startsWith(`${target}/`) : path === target;
};

/** Git state of a set of targets, from the repository status. */
export interface TargetState {
  /** The one selected file, if exactly one file is selected. */
  file: string | null;
  /** Tracked changes (modified, added, deleted, renamed, conflicts) below the targets. */
  changed: string[];
  /** Unversioned files below the targets. */
  unversioned: string[];
  /** The single file is known to git (neither unversioned nor ignored). */
  fileTracked: boolean;
  /** Everything selected is unversioned, so it can go into `.gitignore`. */
  allUnversioned: boolean;
  ignored: boolean;
}

export function targetState(targets: VcsTarget[], entries: StatusEntry[]): TargetState {
  const inTargets = entries.filter((e) => targets.some((t) => covers(t, e.path)));
  const unversioned = inTargets.filter((e) => e.worktree === "untracked").map((e) => e.path);
  const changed = inTargets.filter((e) => e.worktree !== "untracked" && e.worktree !== "ignored").map((e) => e.path);
  const file = targets.length === 1 && !targets[0].dir ? targets[0].path : null;
  const ignored = targets.length > 0 && targets.every((t) => t.ignored);
  return {
    file,
    changed,
    unversioned,
    fileTracked: file !== null && !ignored && !unversioned.includes(projectPathOf(file)),
    // A folder counts when it holds unversioned files and no tracked changes; git status does
    // not list unchanged tracked files, so a folder with both is a best guess.
    allUnversioned:
      targets.length > 0 &&
      !ignored &&
      targets.every((t) => (t.dir ? t.path !== "" && unversioned.some((p) => covers(t, p)) : unversioned.includes(projectPathOf(t.path)))) &&
      changed.length === 0,
    ignored,
  };
}
