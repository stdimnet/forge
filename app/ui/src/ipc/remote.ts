/**
 * Remote development over SSH (`app/src-tauri/src/commands/remote.rs`, `crates/forge-remote`).
 * Connecting starts the Forge agent on the host; the project then opens as a remote
 * backend and every other command (files, git, terminal, LSP, PHP index) runs there.
 */
import { invoke, listen } from "./index";
import type { WorkspaceInfo } from "./workspace";

export type AuthMethod = "password" | "keyPair" | "agent";

/** A saved SSH configuration (Settings | Tools | SSH Configurations). */
export interface SshConfiguration {
  id: string;
  /** Host name, address or a `Host` alias of `~/.ssh/config`. */
  host: string;
  port: number;
  user: string;
  auth: AuthMethod;
  keyFile?: string;
  /** Ask for the password / passphrase every time. */
  dontSaveSecret?: boolean;
  name?: string;
  /** "Parse config file ~/.ssh/config" (on when missing). */
  parseConfig?: boolean;
}

export interface SshConfigView extends SshConfiguration {
  /** `user@host` (`:port` when not 22). */
  label: string;
  /** A password / passphrase is stored in the keychain. */
  hasSecret: boolean;
}

/** A concrete `Host` of `~/.ssh/config`. */
export interface HostEntry {
  alias: string;
  hostName: string;
  user?: string;
  port?: number;
}

export type HostKeyStatus =
  | { status: "trusted" }
  | { status: "unknown" }
  | { status: "changed"; line: number; oldFingerprint: string }
  | { status: "revoked" };

export type HostKeyInfo = HostKeyStatus & {
  host: string;
  port: number;
  /** e.g. `ssh-ed25519` */
  keyType: string;
  /** `SHA256:…` */
  fingerprint: string;
};

export type ConnectError =
  | { kind: "hostKey"; hostKey: HostKeyInfo }
  | { kind: "auth"; message: string }
  | { kind: "network"; message: string }
  | { kind: "other"; message: string };

export interface TestOutcome {
  ok: boolean;
  latencyMs?: number;
  /** `uname -srm` of the host. */
  system?: string;
  error?: ConnectError;
}

export type ConnectOutcome =
  | { status: "connected"; connectionId: string; label: string; home: string; hostname: string; system: string }
  | { status: "failed"; error: ConnectError };

export interface BrowseEntry {
  name: string;
  /** Absolute path on the host. */
  path: string;
  dir: boolean;
  /** Holds a project marker (composer.json, package.json, .git, …). */
  project?: boolean;
}

export interface BrowseResult {
  path: string;
  parent?: string | null;
  entries: BrowseEntry[];
}

export type ConnectionStatus =
  | { state: "connecting"; message: string }
  | { state: "connected"; latencyMs?: number | null }
  | { state: "reconnecting"; attempt: number; error: string; nextRetrySecs: number }
  | { state: "disconnected"; error?: string | null };

export type RemoteStatus = ConnectionStatus & { connectionId: string; label: string; host: string };

export const connectErrorText = (e: ConnectError): string =>
  e.kind === "hostKey" ? `Host key of ${e.hostKey.host} must be confirmed (${e.hostKey.fingerprint})` : e.message;

export const remoteApi = {
  configs: () => invoke<SshConfigView[]>("remote_ssh_configs"),
  saveConfig: (config: SshConfiguration, secret?: string) =>
    invoke<SshConfigView>("remote_ssh_config_save", { config, secret }),
  deleteConfig: (id: string) => invoke<void>("remote_ssh_config_delete", { id }),
  validate: (config: SshConfiguration) => invoke<string[]>("remote_validate_config", { config }),
  sshHosts: () => invoke<HostEntry[]>("remote_ssh_hosts"),
  test: (config: SshConfiguration, secret?: string, acceptFingerprint?: string) =>
    invoke<TestOutcome>("remote_test_connection", { config, secret, acceptFingerprint }),
  connect: (config: SshConfiguration, secret?: string, acceptFingerprint?: string) =>
    invoke<ConnectOutcome>("remote_connect", { config, secret, acceptFingerprint }),
  browse: (connectionId: string, path: string) => invoke<BrowseResult>("remote_browse", { connectionId, path }),
  openProject: (connectionId: string, path: string) =>
    invoke<WorkspaceInfo>("remote_open_project", { connectionId, path }),
  disconnect: (connectionId: string) => invoke<void>("remote_disconnect", { connectionId }),
  status: () => invoke<RemoteStatus | null>("remote_status"),
  reconnect: () => invoke<void>("remote_reconnect"),
  onStatus: (cb: (s: RemoteStatus) => void) => listen<RemoteStatus>("remote://status", cb),
  onProgress: (cb: (p: { message: string }) => void) => listen<{ message: string }>("remote://progress", cb),
};
