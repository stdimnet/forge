/**
 * State of the remote development UI: the New SSH Connection dialog, the host key question,
 * the remote folder chooser and the project's connection status.
 */
import { createRoot, createSignal } from "solid-js";
import {
  remoteApi,
  type BrowseResult,
  type ConnectError,
  type HostKeyInfo,
  type RemoteStatus,
  type SshConfiguration,
} from "@/ipc/remote";
import type { RecentProject, WorkspaceInfo } from "@/ipc/workspace";
import { openRemoteProject } from "@/state/workspace";
import { openDialogWindow } from "@/shell/window/dialogs";

export const blankConfig = (): SshConfiguration => ({
  id: "",
  host: "",
  port: 22,
  user: "",
  auth: "password",
  keyFile: "",
  dontSaveSecret: false,
});

export interface SshDialogRequest {
  config: SshConfiguration;
  /** Project folder to preselect in the second step. */
  path?: string;
  /** Why the dialog opened (a recent project could not connect). */
  error?: ConnectError;
}

export const SSH_CONNECTION_DIALOG = "ssh-connection";

/** Result of the dialog: the remote project it opened (adopted by the opening window). */
export interface SshDialogResult {
  workspace: WorkspaceInfo;
}

/** File | Remote Development | New SSH Connection… (and the Welcome screen). */
export function openNewSshConnection(request: Partial<SshDialogRequest> = {}): void {
  const full: SshDialogRequest = { config: request.config ?? blankConfig(), path: request.path, error: request.error };
  void openDialogWindow(SSH_CONNECTION_DIALOG, full);
}

/** A host key question; resolves with the user's answer. */
export interface HostKeyQuestion {
  info: HostKeyInfo;
  answer: (accept: boolean) => void;
}

export const [hostKeyQuestion, setHostKeyQuestion] = createSignal<HostKeyQuestion | null>(null);

/** Ask whether to trust an unknown or changed host key. */
export function confirmHostKey(info: HostKeyInfo): Promise<boolean> {
  return new Promise((resolve) => {
    setHostKeyQuestion({
      info,
      answer: (accept) => {
        setHostKeyQuestion(null);
        resolve(accept);
      },
    });
  });
}

export interface FolderChooserRequest {
  connectionId: string;
  label: string;
  start: string;
  /** Lists a folder (default: `remote_browse` of the SSH connection; WSL passes its own). */
  browse?: (path: string) => Promise<BrowseResult>;
  pick: (path: string | null) => void;
}

export const [folderChooser, setFolderChooser] = createSignal<FolderChooserRequest | null>(null);

/** Let the user pick a folder on the connected host. */
export function chooseRemoteFolder(
  connectionId: string,
  label: string,
  start: string,
  browse?: (path: string) => Promise<BrowseResult>,
): Promise<string | null> {
  return new Promise((resolve) => {
    setFolderChooser({
      connectionId,
      label,
      start,
      browse,
      pick: (path) => {
        setFolderChooser(null);
        resolve(path);
      },
    });
  });
}

// ---- connection status of the open project ---------------------------------------------------

export const [remoteStatus, setRemoteStatus] = createSignal<RemoteStatus | null>(null);

let listening = false;

/** Follow `remote://status` and fetch the current state (on startup and project changes). */
export function trackRemoteStatus(): void {
  if (!listening) {
    listening = true;
    void remoteApi.onStatus((s) => setRemoteStatus(s));
  }
  void remoteApi
    .status()
    .then((s) => setRemoteStatus(s))
    .catch(() => setRemoteStatus(null));
}

// ---- connecting ---------------------------------------------------------------------------------

export const [connecting, setConnecting] = createSignal<{ label: string; message: string } | null>(null);

createRoot(() => {
  void remoteApi.onProgress((p) => {
    const c = connecting();
    if (c) setConnecting({ ...c, message: p.message });
  });
});

/**
 * Connect with `config` and run the host key question when needed. Returns the connection
 * id, or the error the dialog should show.
 */
export async function connectWithConfirmation(
  config: SshConfiguration,
  secret?: string,
): Promise<{ connectionId: string; home: string; label: string; system: string; hostname: string } | { error: ConnectError }> {
  let accept: string | undefined;
  for (;;) {
    const outcome = await remoteApi.connect(config, secret, accept);
    if (outcome.status === "connected") return outcome;
    const e = outcome.error;
    if (e.kind === "hostKey" && (e.hostKey.status === "unknown" || e.hostKey.status === "changed") && !accept) {
      if (!(await confirmHostKey(e.hostKey))) return { error: e };
      accept = e.hostKey.fingerprint;
      continue;
    }
    return { error: e };
  }
}

type RecentOpener = (project: RecentProject) => Promise<boolean>;
let devEnvOpener: RecentOpener | null = null;

/** Dev container and WSL projects open through their feature (`features/devenv`). */
export function setRecentDevEnvOpener(opener: RecentOpener): void {
  devEnvOpener = opener;
}

/** Open a recent remote project: reconnect with its saved configuration, else ask. */
export async function openRecentRemote(project: RecentProject): Promise<void> {
  const r = project.remote;
  if (!r) return;
  if (r.kind && devEnvOpener && (await devEnvOpener(project))) return;
  const saved = (await remoteApi.configs().catch(() => [])).find((c) => c.id === r.configId);
  const config: SshConfiguration = saved ?? { ...blankConfig(), host: r.host, port: r.port, user: r.user };
  // Without a stored password the dialog asks for it.
  if (!saved || (saved.auth === "password" && !saved.hasSecret)) {
    openNewSshConnection({ config, path: r.path });
    return;
  }
  setConnecting({ label: r.label, message: `Connecting to ${r.label}…` });
  try {
    const result = await connectWithConfirmation(config);
    if ("error" in result) {
      openNewSshConnection({ config, path: r.path, error: result.error });
      return;
    }
    await openRemoteProject(result.connectionId, r.path);
  } finally {
    setConnecting(null);
  }
}
