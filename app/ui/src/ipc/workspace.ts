import type { Framework } from "./project";
import { invoke } from "./index";

export type FileKind = "file" | "dir";

export interface FileEntry {
  name: string;
  path: string;
  kind: FileKind;
  /** Matched by `.gitignore`; the project tree grays these out. */
  ignored?: boolean;
}

/** The host of a project opened over SSH. */
export interface RemoteInfo {
  host: string;
  port: number;
  user: string;
  /** `user@host` (with `:port` when not 22). */
  label: string;
  configId?: string;
  /** Project folder on the host. */
  path: string;
}

export interface WorkspaceInfo {
  name: string;
  /** Local path, or `user@host:/path` for a remote project. */
  root: string;
  remote?: RemoteInfo;
}

export interface RecentProject {
  name: string;
  root: string;
  /** Detected framework (or language) for the Welcome screen badge. */
  primary?: Framework;
  remote?: RemoteInfo;
}

export const workspaceApi = {
  open: (path: string) => invoke<WorkspaceInfo>("workspace_open", { path }),
  current: () => invoke<WorkspaceInfo | null>("workspace_current"),
  recent: () => invoke<RecentProject[]>("workspace_recent"),
  listDir: (path: string) => invoke<FileEntry[]>("fs_list_dir", { path }),
  readFile: (path: string) => invoke<string>("fs_read_file", { path }),
  writeFile: (path: string, contents: string) =>
    invoke<void>("fs_write_file", { path, contents }),
};
