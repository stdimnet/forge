/**
 * Welcome screen > Remote Development > Dev Containers: create a dev container from a local
 * folder with a devcontainer.json, and the dev containers Docker knows with Start / Stop /
 * Rebuild / Delete (click a running one to open it).
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { Box, Hammer, LoaderCircle, Play, Plus, RefreshCw, Square, Trash2 } from "lucide-solid";
import { isTauri } from "@/ipc";
import { devcontainerApi, type DevContainerInstance } from "@/ipc/devenv";
import { connecting } from "@/features/remote/store";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { createDevContainer, openDevContainer } from "./store";

/** Ask for a local folder (browser mode: the demo project). */
async function pickFolder(): Promise<string | null> {
  if (!isTauri()) return (await devcontainerApi.configs()).at(0)?.config?.localFolder ?? null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const dir = await open({ directory: true, multiple: false, title: "Choose the Project Folder with a devcontainer.json" });
  return typeof dir === "string" ? dir : null;
}

export function DevContainersPage(props: { onError: (message: string) => void }) {
  const [list, { refetch }] = createResource(() => devcontainerApi.list());
  const [busy, setBusy] = createSignal<string | null>(null);

  const fail = (e: unknown) => props.onError(String((e as Error)?.message ?? e).replace(/^Error: /, ""));

  const act = async (c: DevContainerInstance, action: "start" | "stop" | "remove") => {
    if (
      action === "remove" &&
      !(await askConfirm({
        title: "Delete Dev Container",
        message: `Delete the dev container "${c.name}" (${c.containerName})? The sources stay on this machine.`,
        confirm: "Delete",
      }))
    )
      return;
    setBusy(c.containerId);
    try {
      await devcontainerApi.action(c.containerId, action);
      await refetch();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(null);
    }
  };

  const open = (c: DevContainerInstance) => void openDevContainer(c.containerId, `Dev Container: ${c.name}`).catch(fail);

  const create = async () => {
    try {
      const folder = await pickFolder();
      if (folder) createDevContainer({ folder });
    } catch (e) {
      fail(e);
    }
  };

  return (
    <div class="welcome-remote" data-testid="welcome-devcontainers">
      <div class="welcome-remote-head">
        <h2>Dev Containers</h2>
        <p class="muted">
          Develop inside a container described by the project's <span class="mono">devcontainer.json</span>. Forge builds
          it with the local Docker CLI, mounts the sources and starts its agent inside, so indexing, language servers,
          terminals and run configurations use the container's tools.
        </p>
      </div>
      <div class="welcome-remote-section">
        <div class="welcome-remote-section-title">
          <Box size={15} />
          <span>Containers</span>
          <button class="icon-btn" title="Refresh" onClick={() => void refetch()}>
            <RefreshCw size={13} />
          </button>
          <button class="btn-primary welcome-remote-new" onClick={() => void create()} data-testid="devcontainer-new">
            <Plus size={14} /> New Dev Container
          </button>
        </div>
        <Show when={connecting()}>
          {(c) => (
            <div class="welcome-remote-connecting">
              <LoaderCircle size={14} class="ssh-spin" /> {c().message}
            </div>
          )}
        </Show>
        <Show when={list.loading && !list()}>
          <div class="muted devc-loading">
            <LoaderCircle size={14} class="ssh-spin" /> Asking Docker…
          </div>
        </Show>
        <Show when={list() && !list()!.available}>
          <div class="ssh-error">Docker is not available: {list()!.error ?? "the Docker CLI did not answer"}</div>
        </Show>
        <Show
          when={list()?.containers.length}
          fallback={
            <Show when={list()?.available}>
              <div class="welcome-remote-empty">
                <Box size={30} class="muted" />
                <p>No dev containers yet.</p>
                <button class="btn-secondary" onClick={() => void create()}>
                  Create Dev Container…
                </button>
              </div>
            </Show>
          }
        >
          <div class="welcome-list devc-list">
            <For each={list()!.containers}>
              {(c) => (
                <div
                  class="welcome-project devc-item"
                  classList={{ stopped: c.state !== "running" }}
                  role="button"
                  tabIndex={0}
                  title={c.state === "running" ? `Open ${c.workspaceFolder} in ${c.name}` : `Start ${c.name} and open it`}
                  onClick={() => open(c)}
                  data-container={c.containerId}
                >
                  <span class="welcome-badge devc-badge">
                    <Box size={16} />
                  </span>
                  <span class="welcome-project-text">
                    <span class="welcome-project-name">
                      {c.name}
                      <span class="devc-state" data-state={c.state}>
                        {c.state === "running" ? "Running" : c.state === "exited" ? "Stopped" : c.state}
                      </span>
                    </span>
                    <span class="welcome-project-path">
                      {c.localFolder} → {c.workspaceFolder}
                    </span>
                    <span class="welcome-project-path devc-detail">
                      {c.containerName} · {c.image} · {c.status}
                    </span>
                  </span>
                  <span class="devc-actions" onClick={(e) => e.stopPropagation()}>
                    <Show when={busy() === c.containerId}>
                      <LoaderCircle size={14} class="ssh-spin" />
                    </Show>
                    <Show
                      when={c.state === "running"}
                      fallback={
                        <button class="icon-btn" title="Start" onClick={() => void act(c, "start")}>
                          <Play size={14} />
                        </button>
                      }
                    >
                      <button class="icon-btn" title="Stop" onClick={() => void act(c, "stop")}>
                        <Square size={13} />
                      </button>
                    </Show>
                    <button
                      class="icon-btn"
                      title="Rebuild"
                      onClick={() => createDevContainer({ folder: c.localFolder, configPath: c.configFile, rebuild: true })}
                    >
                      <Hammer size={14} />
                    </button>
                    <button class="icon-btn" title="Delete" onClick={() => void act(c, "remove")}>
                      <Trash2 size={14} />
                    </button>
                  </span>
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>
    </div>
  );
}
