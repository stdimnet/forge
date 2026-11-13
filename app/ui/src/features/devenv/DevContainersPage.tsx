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
import { t } from "@/i18n";

/** Ask for a local folder (browser mode: the demo project). */
async function pickFolder(): Promise<string | null> {
  if (!isTauri()) return (await devcontainerApi.configs()).at(0)?.config?.localFolder ?? null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const dir = await open({ directory: true, multiple: false, title: t("tools.devenv.chooseTheProjectFolderWith") });
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
        title: t("tools.devenv.deleteDevContainer"),
        message: t("tools.devenv.deleteTheDevContainerThe", { name: c.name, containerName: c.containerName }),
        confirm: t("common.delete"),
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
        <h2>{t("tools.devenv.devContainers")}</h2>
        <p class="muted">
          {t("tools.devenv.developInsideAContainerDescribed")} <span class="mono">devcontainer.json</span>{t("tools.devenv.forgeBuildsItWithThe")}
        </p>
      </div>
      <div class="welcome-remote-section">
        <div class="welcome-remote-section-title">
          <Box size={15} />
          <span>{t("tools.devenv.containers")}</span>
          <button class="icon-btn" title={t("common.refresh")} onClick={() => void refetch()}>
            <RefreshCw size={13} />
          </button>
          <button class="btn-primary welcome-remote-new" onClick={() => void create()} data-testid="devcontainer-new">
            <Plus size={14} /> {t("tools.devenv.newDevContainer")}
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
            <LoaderCircle size={14} class="ssh-spin" /> {t("tools.devenv.askingDockerEllipsis")}
          </div>
        </Show>
        <Show when={list() && !list()!.available}>
          <div class="ssh-error">{t("tools.devenv.dockerIsNotAvailable", { value: list()!.error ?? t("tools.devenv.theDockerCliDidNot") })}</div>
        </Show>
        <Show
          when={list()?.containers.length}
          fallback={
            <Show when={list()?.available}>
              <div class="welcome-remote-empty">
                <Box size={30} class="muted" />
                <p>{t("tools.devenv.noDevContainersYet")}</p>
                <button class="btn-secondary" onClick={() => void create()}>
                  {t("tools.devenv.createDevContainerEllipsis")}
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
                  title={c.state === "running" ? t("tools.devenv.openIn", { workspaceFolder: c.workspaceFolder, name: c.name }) : t("tools.devenv.startAndOpenIt", { name: c.name })}
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
                        {c.state === "running" ? t("tools.devenv.running") : c.state === "exited" ? t("tools.devenv.stopped") : c.state}
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
                        <button class="icon-btn" title={t("tools.devenv.start")} onClick={() => void act(c, "start")}>
                          <Play size={14} />
                        </button>
                      }
                    >
                      <button class="icon-btn" title={t("common.stop")} onClick={() => void act(c, "stop")}>
                        <Square size={13} />
                      </button>
                    </Show>
                    <button
                      class="icon-btn"
                      title={t("tools.devenv.rebuild")}
                      onClick={() => createDevContainer({ folder: c.localFolder, configPath: c.configFile, rebuild: true })}
                    >
                      <Hammer size={14} />
                    </button>
                    <button class="icon-btn" title={t("common.delete")} onClick={() => void act(c, "remove")}>
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
