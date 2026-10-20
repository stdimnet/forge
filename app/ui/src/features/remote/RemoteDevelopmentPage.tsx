/**
 * Welcome screen > Remote Development: recent remote projects, saved SSH connections and
 * "New Connection" (like PHPStorm's Remote Development > SSH page).
 */
import { createResource, For, Show } from "solid-js";
import { LoaderCircle, Plus, Server, Settings } from "lucide-solid";
import { remoteApi } from "@/ipc/remote";
import type { RecentProject } from "@/ipc/workspace";
import { openSettings } from "@/features/settings/dialogState";
import { AUTH_LABELS } from "./SshForm";
import { connecting, openNewSshConnection, openRecentRemote } from "./store";

/** The SSH badge shown on remote projects. */
export function SshBadge() {
  return (
    <span class="ssh-badge" title="Remote project (SSH)">
      SSH
    </span>
  );
}

export function RemoteDevelopmentPage(props: { recent: RecentProject[]; row: (p: RecentProject) => unknown }) {
  const [configs] = createResource(() => remoteApi.configs().catch(() => []));
  const remoteProjects = () => props.recent.filter((p) => p.remote);

  return (
    <div class="welcome-remote" data-testid="welcome-remote">
      <div class="welcome-remote-head">
        <div>
          <h2>Remote Development</h2>
          <p class="muted">
            Open a project on another machine over SSH. Forge starts its agent there, so indexing, Intelephense and other
            language servers, git, terminals and run configurations work next to the files.
          </p>
        </div>
      </div>

      <div class="welcome-remote-section">
        <div class="welcome-remote-section-title">
          <Server size={15} />
          <span>SSH</span>
          <button class="btn-primary welcome-remote-new" onClick={() => openNewSshConnection()} data-testid="ssh-new">
            <Plus size={14} /> New Connection
          </button>
        </div>

        <Show when={connecting()}>
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
              <p>No remote connections yet.</p>
              <button class="btn-secondary" onClick={() => openNewSshConnection()}>
                New SSH Connection…
              </button>
            </div>
          }
        >
          <Show when={remoteProjects().length}>
            <div class="welcome-remote-group">Recent projects</div>
            <div class="welcome-list">
              <For each={remoteProjects()}>{(p) => <>{props.row(p)}</>}</For>
            </div>
          </Show>
          <Show when={configs()?.length}>
            <div class="welcome-remote-group">
              Connections
              <button class="icon-btn welcome-remote-gear" title="SSH Configurations…" onClick={() => openSettings("tools.ssh")}>
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
                      openNewSshConnection({ config });
                    }}
                    title={`Connect to ${c.label}`}
                  >
                    <span class="welcome-badge ssh-host-badge">
                      <Server size={16} />
                    </span>
                    <span class="welcome-project-text">
                      <span class="welcome-project-name">{c.name || c.label}</span>
                      <span class="welcome-project-path">{AUTH_LABELS[c.auth]}</span>
                    </span>
                    <span class="welcome-project-type">Connect…</span>
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
