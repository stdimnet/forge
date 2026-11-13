/**
 * Remote Host tool window (`app/src-tauri/src/commands/remote_host.rs`,
 * `forge_deploy::remote_host`): the files of a deployment server as a lazy tree, its context
 * menu and Edit Remote File (a temporary copy in the user's cache folder, uploaded back on
 * save with a conflict check). Transfers and comparisons use `deploymentApi` with the
 * node's mapped project path.
 */
import { invoke } from "./index";

/** A file or folder of the remote tree (`forge_deploy::browse::RemoteNode`). */
export interface RemoteNode {
  /** Relative to the server's root path ("" is the root). */
  path: string;
  name: string;
  dir: boolean;
  size: number;
  modified: number | null;
  /** The project path this entry is mapped to, null when no mapping covers it. */
  local: string | null;
  /** The deployment path of a mapping (highlighted). */
  mappingRoot: boolean;
  /** Excluded on the deployment side. */
  excluded: boolean;
}

export interface RemoteListing {
  /** The server's absolute root folder. */
  root: string;
  /** The root folder as a node (its mapping), named after `root`. */
  rootNode: RemoteNode;
  nodes: RemoteNode[];
}

export interface EditOpened {
  text: string;
  /** The temporary copy on this machine. */
  temp: string;
  /** Where it is on the server. */
  remote: string;
}

export type SaveOutcome = { status: "uploaded" } | { status: "conflict"; deleted: boolean };

export const remoteHostApi = {
  list: (serverId: string, path: string) => invoke<RemoteListing>("remote_host_list", { serverId, path }),
  create: (serverId: string, dir: string, name: string, folder: boolean) =>
    invoke<RemoteNode>("remote_host_create", { serverId, dir, name, folder }),
  rename: (serverId: string, path: string, newName: string) => invoke<string>("remote_host_rename", { serverId, path, newName }),
  delete: (serverId: string, paths: string[]) => invoke<number>("remote_host_delete", { serverId, paths }),
  edit: (serverId: string, path: string) => invoke<EditOpened>("remote_host_edit", { serverId, path }),
  saveEdit: (serverId: string, path: string, text: string, force = false) =>
    invoke<SaveOutcome>("remote_host_save_edit", { serverId, path, text, force }),
  closeEdit: (serverId: string, path: string) => invoke<void>("remote_host_close_edit", { serverId, path }),
  disconnect: (serverId?: string) => invoke<void>("remote_host_disconnect", { serverId }),
};
