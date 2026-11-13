import { createSignal } from "solid-js";

/**
 * Minimal VCS state the shell needs (branch widget, file status colors). The Git feature
 * fills it; without it the shell shows neutral defaults.
 */
export type FileStatus = "modified" | "added" | "deleted" | "untracked" | "conflict";

/** The branch of the open project's repository; empty without a repository. */
const [currentBranch, setCurrentBranch] = createSignal("");
const [fileStatuses, setFileStatuses] = createSignal<Record<string, FileStatus>>({});

/** An operation in progress replaces the branch name in the widgets: "Rebasing main". */
const [branchOperation, setBranchOperation] = createSignal<string | null>(null);

export { currentBranch, setCurrentBranch, fileStatuses, setFileStatuses, branchOperation, setBranchOperation };

/** Label of the branch widgets (toolbar, status bar); empty when the project has no repository. */
export const branchLabel = (): string => branchOperation() ?? currentBranch();

/** Forget the repository state of a project that closes (before the next one's is read). */
export function resetVcsState(): void {
  setCurrentBranch("");
  setFileStatuses({});
  setBranchOperation(null);
}

export const statusOf = (path: string): FileStatus | undefined => fileStatuses()[path];

/** A directory holds files with unresolved merge conflicts (shown red like the files). */
export const dirHasConflicts = (dir: string): boolean =>
  Object.entries(fileStatuses()).some(([p, s]) => s === "conflict" && p.startsWith(`${dir}/`));

/** Status for a directory: modified if any descendant has a status. */
export const dirHasChanges = (dir: string): boolean =>
  Object.keys(fileStatuses()).some((p) => p.startsWith(`${dir}/`));
