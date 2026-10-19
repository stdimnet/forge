import { invoke } from "./index";

export type FileKind = "file" | "dir";

export interface FileEntry {
  name: string;
  path: string;
  kind: FileKind;
}

export interface WorkspaceInfo {
  name: string;
  root: string;
}

export const workspaceApi = {
  open: (path: string) => invoke<WorkspaceInfo>("workspace_open", { path }),
  listDir: (path: string) => invoke<FileEntry[]>("fs_list_dir", { path }),
  readFile: (path: string) => invoke<string>("fs_read_file", { path }),
  writeFile: (path: string, contents: string) =>
    invoke<void>("fs_write_file", { path, contents }),
};
