/**
 * Remote development over SSH: the New SSH Connection dialog with the host key question
 * and the remote folder chooser, the connection status bar widget and Settings | Tools |
 * SSH Configurations. The backend (`crates/forge-remote`, `crates/forge-remote-agent`)
 * runs the project's services on the host; the rest of the UI works unchanged.
 */
import { createEffect, createRoot, on } from "solid-js";
import { render } from "solid-js/web";
import { registerStatusItem } from "@/features/statusItems";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerCommand } from "@/state/commands";
import { isRemoteProject, workspace } from "@/state/workspace";
import { remoteApi } from "@/ipc/remote";
import { balloonsFor } from "@/keymap";
import { openSettings } from "@/features/settings/dialogState";
import { HostKeyDialog } from "./HostKeyDialog";
import { RemoteFolderChooser } from "./RemoteFolderChooser";
import { RemoteStatusItem } from "./RemoteStatusItem";
import { SshConfigurationsPage } from "./SshConfigurationsPage";
import { SshConnectionDialog } from "./SshConnectionDialog";
import {
  openNewSshConnection,
  SSH_CONNECTION_DIALOG,
  trackRemoteStatus,
  type SshDialogRequest,
  type SshDialogResult,
} from "./store";
import { registerDialog } from "@/shell/window/dialogs";
import { isDialogWindow } from "@/shell/window/mode";
import { adoptWorkspace } from "@/state/workspace";
import "./remote.css";
import { t } from "@/i18n";

const showBalloon = balloonsFor("Remote Development");

export { openNewSshConnection };

registerStatusItem({ id: "remote.connection", align: "right", order: -30, component: RemoteStatusItem, when: isRemoteProject });

registerSettingsPage({
  id: "tools.ssh",
  title: t("settings.page.tools.ssh"),
  group: "Tools",
  order: 30,
  keywords: ["SSH", "remote", "host", "password", "private key", "known_hosts", "Gateway"],
  component: SshConfigurationsPage,
});

registerCommand({
  id: "Remote.NewSshConnection",
  title: "New SSH Connection…",
  category: "File",
  run: () => openNewSshConnection(),
});

registerDialog<SshDialogRequest, SshDialogResult>({
  id: SSH_CONNECTION_DIALOG,
  title: t("tools.remote.newSshConnection"),
  width: 640,
  height: 560,
  minWidth: 520,
  minHeight: 420,
  component: SshConnectionDialog,
  // The dialog opened a remote project in the backend: show it here.
  onClosed: (result) => {
    if (result?.workspace) adoptWorkspace(result.workspace);
  },
});

registerCommand({
  id: "remote.settings",
  title: "SSH Configurations…",
  category: "Settings",
  run: () => openSettings("tools.ssh"),
});

registerCommand({
  id: "Remote.Reconnect",
  title: "Reconnect to Remote Host",
  category: "File",
  run: async () => {
    try {
      await remoteApi.reconnect();
    } catch (e) {
      showBalloon(t("tools.remote.cannotReconnect"), String((e as Error)?.message ?? e));
    }
  },
});

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "remote-overlays";
  document.body.appendChild(host);
  render(
    () => (
      <>
        <RemoteFolderChooser />
        <HostKeyDialog />
      </>
    ),
    host,
  );
  // The status widget follows the open project's connection.
  if (!isDialogWindow()) createRoot(() => createEffect(on(() => workspace()?.root, () => trackRemoteStatus())));
}
