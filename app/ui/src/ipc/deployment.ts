/**
 * Deployment (`app/src-tauri/src/commands/deployment.rs`, `crates/forge-deploy`): servers of
 * the project (`.forge/deployment.toml`), uploads, downloads, Sync with Deployed, Compare with
 * Deployed Version and automatic upload on save. Passwords live in the OS keychain.
 */
import { invoke, listen } from "./index";
import type { DirCompare } from "@/features/compare/api";

export type ServerKind = "sftp" | "ftp" | "ftps" | "local";

export interface Mapping {
  /** Project-relative folder, "" for the project root. */
  local: string;
  /** Relative to the root path. */
  deployment: string;
  /** Relative to the web server URL. */
  web: string;
}

export interface ExcludedPath {
  side: "local" | "deployment";
  path: string;
}

/** A deployment server (`forge_deploy::DeployServer`). */
export interface DeployServer {
  id: string;
  name: string;
  kind: ServerKind;
  /** SFTP: id of the SSH configuration. */
  sshConfig: string;
  host: string;
  port: number | null;
  user: string;
  anonymous: boolean;
  passive: boolean;
  rootPath: string;
  webUrl: string;
  mappings: Mapping[];
  excluded: ExcludedPath[];
}

export interface ServerView extends DeployServer {
  label: string;
  hasSecret: boolean;
}

export interface DeployConfig {
  defaultServer: string | null;
  servers: ServerView[];
  /** Transfers work (a project on this machine). */
  available: boolean;
}

export interface TestResult {
  ok: boolean;
  message: string;
  root?: string | null;
}

export type Direction = "upload" | "download";

export type FileStatus = "transferred" | "upToDate" | "newerOnTarget" | "excluded" | "notMapped" | "deleted" | "failed";

export interface FileResult {
  local: string;
  remote: string;
  dir: boolean;
  status: FileStatus;
  bytes: number;
  message?: string;
}

export interface Summary {
  results: FileResult[];
  transferred: number;
  skipped: number;
  deleted: number;
  failed: number;
  bytes: number;
  newerOnTarget: string[];
  cancelled: boolean;
}

export type TransferEvent =
  | { kind: "started"; id: string; server: string; operation: string; paths: string[]; automatic: boolean }
  | { kind: "file"; id: string; file: FileResult }
  | { kind: "finished"; id: string; summary: Summary; error: string | null };

export interface SyncCompare {
  local: string;
  remote: string;
  compare: DirCompare;
}

export interface RemoteFile {
  text: string | null;
  binary: boolean;
  remote: string;
  server: string;
}

export const TRANSFER_EVENT = "deploy://transfer";

export const deploymentApi = {
  config: () => invoke<DeployConfig>("deploy_config"),
  saveServer: (server: DeployServer, secret?: string) => invoke<ServerView>("deploy_server_save", { server, secret }),
  deleteServer: (id: string) => invoke<void>("deploy_server_delete", { id }),
  setDefault: (id: string | null) => invoke<void>("deploy_set_default", { id }),
  validate: (server: DeployServer) => invoke<string[]>("deploy_validate", { server }),
  test: (server: DeployServer, secret?: string) => invoke<TestResult>("deploy_test", { server, secret }),
  transfer: (id: string, serverId: string | null, direction: Direction, paths: string[], overwriteNewer = false) =>
    invoke<Summary>("deploy_transfer", { id, serverId, direction, paths, overwriteNewer }),
  delete: (id: string, serverId: string | null, paths: string[]) => invoke<Summary>("deploy_delete", { id, serverId, paths }),
  cancel: (id: string) => invoke<void>("deploy_cancel", { id }),
  autoUpload: (id: string, paths: string[], explicit: boolean) => invoke<Summary | null>("deploy_auto_upload", { id, paths, explicit }),
  syncCompare: (serverId: string | null, path: string) => invoke<SyncCompare>("deploy_sync_compare", { serverId, path }),
  readRemote: (serverId: string | null, path: string) => invoke<RemoteFile>("deploy_read_remote", { serverId, path }),
  onTransfer: (cb: (e: TransferEvent) => void) => listen<TransferEvent>(TRANSFER_EVENT, cb),
};

/** A new, empty server of a type. */
export function blankServer(kind: ServerKind = "sftp"): DeployServer {
  return {
    id: "",
    name: "",
    kind,
    sshConfig: "",
    host: "",
    port: null,
    user: "",
    anonymous: false,
    passive: true,
    rootPath: "",
    webUrl: "",
    mappings: [{ local: "", deployment: "/", web: "" }],
    excluded: [],
  };
}
