/**
 * Welcome screen > Remote Development > WSL: the distributions of `wsl.exe -l -v`; choosing
 * one starts the Forge agent in it and asks for the project folder. Projects under
 * `\\wsl$\<distro>\…` opened from disk go there too.
 */
import { createResource, For, Show } from "solid-js";
import { LoaderCircle, RefreshCw, SquareTerminal } from "lucide-solid";
import { wslApi, type WslDistro } from "@/ipc/devenv";
import type { RecentProject } from "@/ipc/workspace";
import { connecting } from "@/features/remote/store";
import { openWslProject } from "./store";

export function WslPage(props: { recent: RecentProject[]; row: (p: RecentProject) => unknown; onError: (message: string) => void }) {
  const [info, { refetch }] = createResource(() => wslApi.distributions());
  const recent = () => props.recent.filter((p) => p.remote?.kind?.type === "wsl");
  const open = (d: WslDistro) =>
    void openWslProject(d.name).catch((e) => props.onError(String((e as Error)?.message ?? e).replace(/^Error: /, "")));

  return (
    <div class="welcome-remote" data-testid="welcome-wsl">
      <div class="welcome-remote-head">
        <h2>WSL</h2>
        <p class="muted">
          Open a project inside a Windows Subsystem for Linux distribution. Forge runs its agent in the distribution, so
          files are read at Linux speed and PHP, Composer, Node.js, git and language servers are the distribution's.
        </p>
      </div>
      <div class="welcome-remote-section">
        <div class="welcome-remote-section-title">
          <SquareTerminal size={15} />
          <span>Distributions</span>
          <button class="icon-btn welcome-remote-new" title="Refresh" onClick={() => void refetch()}>
            <RefreshCw size={13} />
          </button>
        </div>
        <Show when={connecting()}>
          {(c) => (
            <div class="welcome-remote-connecting">
              <LoaderCircle size={14} class="ssh-spin" /> {c().message}
            </div>
          )}
        </Show>
        <Show when={info.loading && !info()}>
          <div class="muted devc-loading">
            <LoaderCircle size={14} class="ssh-spin" /> Asking wsl.exe…
          </div>
        </Show>
        <Show when={info() && !info()!.available}>
          <div class="welcome-remote-empty">
            <SquareTerminal size={30} class="muted" />
            <p>{info()!.error ?? "WSL is not installed."}</p>
            <p class="muted ssh-small">Install it on Windows with <span class="mono">wsl --install</span>.</p>
          </div>
        </Show>
        <Show when={info()?.available && info()!.distros.length === 0}>
          <div class="welcome-remote-empty">
            <SquareTerminal size={30} class="muted" />
            <p>{info()!.error ?? "No distributions are installed."}</p>
          </div>
        </Show>
        <Show when={recent().length}>
          <div class="welcome-remote-group">Recent projects</div>
          <div class="welcome-list">
            <For each={recent()}>{(p) => <>{props.row(p)}</>}</For>
          </div>
        </Show>
        <Show when={info()?.distros.length}>
          <div class="welcome-remote-group">Choose a distribution</div>
          <div class="welcome-list" data-testid="wsl-distros">
            <For each={info()!.distros}>
              {(d) => (
                <button class="welcome-project" onClick={() => open(d)} title={`Choose a folder in ${d.name}`} data-distro={d.name}>
                  <span class="welcome-badge devc-badge wsl-badge">
                    <SquareTerminal size={16} />
                  </span>
                  <span class="welcome-project-text">
                    <span class="welcome-project-name">
                      {d.name}
                      <Show when={d.default}>
                        <span class="devc-state">Default</span>
                      </Show>
                    </span>
                    <span class="welcome-project-path">
                      WSL {d.version} · {d.state}
                    </span>
                  </span>
                  <span class="welcome-project-type">Open Folder…</span>
                </button>
              )}
            </For>
          </div>
        </Show>
      </div>
    </div>
  );
}
