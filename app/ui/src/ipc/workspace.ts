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

/** The host of the Code With Me session whose project this is (this window is a guest). */
export interface CollabInfo {
  /** The host's user name. */
  host: string;
  /** `name (machine)`. */
  label: string;
}

export interface WorkspaceInfo {
  name: string;
  /** Local path, `user@host:/path` for a remote project, `cwm://host/name` for a guest. */
  root: string;
  remote?: RemoteInfo;
  /** Set for the host's project in a Code With Me session. */
  collab?: CollabInfo;
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
  createFile: (path: string) => invoke<void>("fs_create_file", { path }),
  createDir: (path: string) => invoke<void>("fs_create_dir", { path }),
  rename: (from: string, to: string) => invoke<void>("fs_rename", { from, to }),
  /** Deletes a file, or a folder with its content. Ask the user first. */
  delete: (path: string) => invoke<void>("fs_delete", { path }),
};
