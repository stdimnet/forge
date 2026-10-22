import { createSignal } from "solid-js";

/**
 * Minimal VCS state the shell needs (branch widget, file status colors). The Git feature
 * fills it; without it the shell shows neutral defaults.
 */
export type FileStatus = "modified" | "added" | "deleted" | "untracked" | "conflict";

const [currentBranch, setCurrentBranch] = createSignal("main");
const [fileStatuses, setFileStatuses] = createSignal<Record<string, FileStatus>>({});

export { currentBranch, setCurrentBranch, fileStatuses, setFileStatuses };

export const statusOf = (path: string): FileStatus | undefined => fileStatuses()[path];

/** A directory holds files with unresolved merge conflicts (shown red like the files). */
export const dirHasConflicts = (dir: string): boolean =>
  Object.entries(fileStatuses()).some(([p, s]) => s === "conflict" && p.startsWith(`${dir}/`));

/** Status for a directory: modified if any descendant has a status. */
export const dirHasChanges = (dir: string): boolean =>
  Object.keys(fileStatuses()).some((p) => p.startsWith(`${dir}/`));
