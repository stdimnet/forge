/** Typed wrappers for Compare Directories (`app/src-tauri/src/commands/compare.rs`). */
import { invoke } from "@/ipc";

export type DiffStatus = "equal" | "different" | "leftOnly" | "rightOnly";

/** One row of the folder diff (`forge_diff::DirDiffEntry`). */
export interface DirDiffEntry {
  /** Path relative to the compared folders. */
  path: string;
  dir: boolean;
  /** Nesting level, 0 for direct children of the compared folders. */
  depth: number;
  status: DiffStatus;
  leftSize: number | null;
  rightSize: number | null;
  /** Unix seconds. */
  leftModified: number | null;
  rightModified: number | null;
}

export interface DirCompare {
  /** Tree order: folders before files, children after their folder. */
  entries: DirDiffEntry[];
  /** The folders had more entries than were read. */
  truncated: boolean;
}

export const compareApi = {
  /** The folder diff of two project folders, computed where the files are. */
  dirs: (left: string, right: string) => invoke<DirCompare>("compare_dirs", { left, right }),
  /** Copy a file or folder over another one (folders are merged). */
  copy: (from: string, to: string, dir: boolean) => invoke<void>("compare_copy", { from, to, dir }),
};
