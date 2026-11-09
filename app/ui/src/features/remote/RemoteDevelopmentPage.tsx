/**
 * Welcome screen > Remote Development: recent remote projects, saved SSH connections and
 * "New Connection" (like PHPStorm's Remote Development > SSH page).
 */
import { createResource, For, Show } from "solid-js";
import { LoaderCircle, Plus, Server, Settings } from "lucide-solid";
import { remoteApi } from "@/ipc/remote";
import type { RecentProject, RemoteKind } from "@/ipc/workspace";
import { openSettings } from "@/features/settings/dialogState";
import { AUTH_LABELS } from "./SshForm";
import { isDialogOpen } from "@/shell/window/dialogs";
import { connecting, openNewSshConnection, openRecentRemote, SSH_CONNECTION_DIALOG } from "./store";
import { t } from "@/i18n";

/** The badge of a remote project: SSH, WSL or Container. */
export function SshBadge(props: { kind?: RemoteKind }) {
  const text = () => (props.kind?.type === "wsl" ? "WSL" : props.kind?.type === "devContainer" ? "Container" : "SSH");
  const title = () =>
    props.kind?.type === "wsl" ? "Project in WSL" : props.kind?.type === "devContainer" ? "Project in a dev container" : "Remote project (SSH)";
  return (
    <span class="ssh-badge" title={title()}>
      {text()}
    </span>
  );
}

export function RemoteDevelopmentPage(props: { recent: RecentProject[]; row: (p: RecentProject) => unknown }) {
  const [configs] = createResource(() => remoteApi.configs().catch(() => []));
  // Dev container and WSL projects are on their own pages.
  const remoteProjects = () => props.recent.filter((p) => p.remote && !p.remote.kind);

  return (
    <div class="welcome-remote" data-testid="welcome-remote">
      <div class="welcome-remote-head">
        <div>
          <h2>{t("tools.remote.remoteDevelopment")}</h2>
          <p class="muted">
            {t("tools.remote.openAProjectOnAnother")}
          </p>
        </div>
      </div>

      <div class="welcome-remote-section">
        <div class="welcome-remote-section-title">
          <Server size={15} />
          <span>SSH</span>
          <button class="btn-primary welcome-remote-new" onClick={() => void openNewSshConnection()} data-testid="ssh-new">
            <Plus size={14} /> {t("tools.remote.newConnection")}
          </button>
        </div>

        <Show when={!isDialogOpen(SSH_CONNECTION_DIALOG) && connecting()}>
          {(c) => (
            <div class="welcome-remote-connecting">
              <LoaderCircle size={14} class="ssh-spin" /> {c().message}
            </div>
          )}
        </Show>

        <Show
          when={remoteProjects().length || configs()?.length}
          fallback={
            <div class="welcome-remote-empty">
              <Server size={30} class="muted" />
              <p>{t("tools.remote.noRemoteConnectionsYet")}</p>
              <button class="btn-secondary" onClick={() => void openNewSshConnection()}>
                {t("tools.remote.newSshConnectionEllipsis")}
              </button>
            </div>
          }
        >
          <Show when={remoteProjects().length}>
            <div class="welcome-remote-group">{t("tools.remote.recentProjects")}</div>
            <div class="welcome-list">
              <For each={remoteProjects()}>{(p) => <>{props.row(p)}</>}</For>
            </div>
          </Show>
          <Show when={configs()?.length}>
            <div class="welcome-remote-group">
              {t("tools.remote.connections")}
              <button class="icon-btn welcome-remote-gear" title={t("tools.remote.sshConfigurationsEllipsis")} onClick={() => openSettings("tools.ssh")}>
                <Settings size={13} />
              </button>
            </div>
            <div class="welcome-list">
              <For each={configs()}>
                {(c) => (
                  <button
                    class="welcome-project"
                    onClick={() => {
                      const { label: _l, hasSecret: _h, ...config } = c;
                      void openNewSshConnection({ config });
                    }}
                    title={t("tools.remote.connectTo", { label: c.label })}
                  >
                    <span class="welcome-badge ssh-host-badge">
                      <Server size={16} />
                    </span>
                    <span class="welcome-project-text">
                      <span class="welcome-project-name">{c.name || c.label}</span>
                      <span class="welcome-project-path">{AUTH_LABELS[c.auth]}</span>
                    </span>
                    <span class="welcome-project-type">{t("tools.remote.connectEllipsis")}</span>
                  </button>
                )}
              </For>
            </div>
          </Show>
        </Show>
      </div>
    </div>
  );
}

export { openRecentRemote };
