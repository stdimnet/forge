/**
 * "Create Dev Container and Mount Sources" (PHPStorm's dev container dialog): pick a
 * `devcontainer.json` of the folder, review what it does, then build or start the container
 * with the Docker CLI while the build log streams, and open the project in it.
 */
import { createEffect, createResource, createSignal, For, Match, on, Show, Switch } from "solid-js";
import { Box, CircleAlert, CircleCheck, LoaderCircle, X } from "lucide-solid";
import { devcontainerApi, type DevContainerConfigView, type LifecycleCommand } from "@/ipc/devenv";
import { trustBeforeOpen } from "@/state/workspace";
import {
  buildLog,
  devContainerDialog,
  listenToBuildLog,
  openDevContainer,
  setBuildLog,
  setDevContainerDialog,
  type BuildState,
  type DevContainerDialogRequest,
} from "./store";

export function DevContainerDialog() {
  return <Show when={devContainerDialog()}>{(req) => <Dialog request={req()} />}</Show>;
}

const lifecycle = (c?: LifecycleCommand): string | undefined =>
  c === undefined
    ? undefined
    : typeof c === "string"
      ? c
      : Array.isArray(c)
        ? c.join(" ")
        : Object.entries(c)
            .map(([k, v]) => `${k}: ${Array.isArray(v) ? v.join(" ") : v}`)
            .join("; ");

function Dialog(props: { request: DevContainerDialogRequest }) {
  const [configs] = createResource(() => props.request.folder, (f) => devcontainerApi.configs(f));
  const [selected, setSelected] = createSignal<string | undefined>(props.request.configPath);
  const [state, setState] = createSignal<BuildState>("idle");
  const [error, setError] = createSignal<string | null>(null);
  let logEl: HTMLDivElement | undefined;

  createEffect(() => {
    const list = configs();
    if (list?.length && !list.some((c) => c.path === selected())) setSelected(list[0].path);
  });
  const current = (): DevContainerConfigView | undefined => configs()?.find((c) => c.path === selected());

  // Keep the newest log line in view.
  createEffect(on(buildLog, () => queueMicrotask(() => logEl && (logEl.scrollTop = logEl.scrollHeight))));

  const close = () => {
    if (state() === "building") void devcontainerApi.cancel();
    setDevContainerDialog(null);
  };

  const build = async () => {
    const cfg = current();
    if (!cfg?.config) return;
    setError(null);
    // The image build and postCreateCommand run the project's code.
    const decision = await trustBeforeOpen(props.request.folder);
    if (decision === "cancel") return;
    if (decision === "safe") {
      setError("Building a dev container runs the project's code (image build, postCreateCommand). Trust the project to build it.");
      return;
    }
    listenToBuildLog();
    setBuildLog([]);
    setState("building");
    try {
      const outcome = await devcontainerApi.up(props.request.folder, cfg.path, props.request.rebuild ?? false);
      setState("opening");
      await openDevContainer(outcome.containerId, `Dev Container: ${cfg.config.name}`);
      setState("done");
      setDevContainerDialog(null);
    } catch (e) {
      setState("failed");
      setError(String((e as Error)?.message ?? e).replace(/^Error: /, ""));
    }
  };

  const title = () => (props.request.rebuild ? "Rebuild Dev Container" : "Create Dev Container and Mount Sources");

  return (
    <div class="ssh-overlay" onMouseDown={(e) => e.target === e.currentTarget && state() !== "building" && close()}>
      <div
        class="ssh-dialog devc-dialog"
        role="dialog"
        aria-label={title()}
        data-testid="devcontainer-dialog"
        onKeyDown={(e) => e.key === "Escape" && state() !== "building" && close()}
      >
        <div class="ssh-titlebar">
          <span>{title()}</span>
          <button class="icon-btn" title="Close" onClick={close}>
            <X size={15} />
          </button>
        </div>
        <div class="ssh-body devc-body">
          <div class="devc-row">
            <span class="devc-label">Sources</span>
            <span class="mono devc-value">{props.request.folder}</span>
          </div>
          <Switch>
            <Match when={configs.loading}>
              <div class="muted">
                <LoaderCircle size={14} class="ssh-spin" /> Looking for devcontainer.json…
              </div>
            </Match>
            <Match when={configs.error}>
              <div class="ssh-error">{String((configs.error as Error)?.message ?? configs.error)}</div>
            </Match>
            <Match when={configs() && configs()!.length === 0}>
              <div class="ssh-error">
                No devcontainer.json in this folder. Add <span class="mono">.devcontainer/devcontainer.json</span> to
                describe the container.
              </div>
            </Match>
            <Match when={state() === "idle" || (state() === "failed" && buildLog().length === 0)}>
              <div class="devc-row">
                <span class="devc-label">Configuration</span>
                <select
                  class="ssh-input devc-select"
                  value={selected() ?? ""}
                  onChange={(e) => setSelected(e.currentTarget.value)}
                  data-testid="devcontainer-config"
                >
                  <For each={configs()}>{(c) => <option value={c.path}>{c.config?.name ? `${c.config.name} (${c.rel})` : c.rel}</option>}</For>
                </select>
              </div>
              <Show when={current()?.error}>{(e) => <div class="ssh-error">{e()}</div>}</Show>
              <Show when={current()?.config}>
                {(c) => (
                  <dl class="devc-summary" data-testid="devcontainer-summary">
                    <dt>Container</dt>
                    <dd>{current()?.source}</dd>
                    <dt>Workspace</dt>
                    <dd class="mono">{c().workspaceFolder}</dd>
                    <dt>Remote user</dt>
                    <dd>{c().remoteUser ?? "(the image's user)"}</dd>
                    <Show when={c().forwardPorts.length}>
                      <dt>Forwarded ports</dt>
                      <dd>{c().forwardPorts.map((p) => (p.host ? `${p.host}:${p.port}` : p.port)).join(", ")}</dd>
                    </Show>
                    <Show when={lifecycle(c().postCreateCommand)}>
                      {(cmd) => (
                        <>
                          <dt>postCreateCommand</dt>
                          <dd class="mono">{cmd()}</dd>
                        </>
                      )}
                    </Show>
                    <Show when={lifecycle(c().postStartCommand)}>
                      {(cmd) => (
                        <>
                          <dt>postStartCommand</dt>
                          <dd class="mono">{cmd()}</dd>
                        </>
                      )}
                    </Show>
                    <Show when={c().features.length}>
                      <dt>Features</dt>
                      <dd>
                        {c().features.map((f) => f.id).join(", ")} <span class="muted">(listed, not installed by Forge)</span>
                      </dd>
                    </Show>
                  </dl>
                )}
              </Show>
              <p class="muted devc-note">
                Forge builds the image with the Docker CLI, mounts the sources and starts its agent in the container:
                indexing, language servers, terminals and run configurations work inside it. The build and the lifecycle
                commands run the project's code.
              </p>
            </Match>
            <Match when={true}>
              <div class="devc-log mono" ref={logEl} data-testid="devcontainer-log">
                <For each={buildLog()}>
                  {(l) => (
                    <div class={`devc-log-line devc-log-${l.kind}${l.kind === "output" ? ` devc-log-${l.stream}` : ""}`}>
                      {l.kind === "command" ? `$ ${l.text}` : l.kind === "warning" ? `Warning: ${l.text}` : l.text.replace(/\n$/, "")}
                    </div>
                  )}
                </For>
              </div>
            </Match>
          </Switch>
          <Show when={error()}>
            {(e) => (
              <div class="ssh-error" data-testid="devcontainer-error">
                {e()}
              </div>
            )}
          </Show>
        </div>
        <div class="ssh-footer">
          <span class="ssh-progress muted ssh-small devc-status">
            <Switch>
              <Match when={state() === "building"}>
                <LoaderCircle size={13} class="ssh-spin" /> {[...buildLog()].reverse().find((l) => l.kind === "step")?.text ?? "Starting…"}
              </Match>
              <Match when={state() === "opening"}>
                <LoaderCircle size={13} class="ssh-spin" /> Starting the Forge agent in the container…
              </Match>
              <Match when={state() === "done"}>
                <CircleCheck size={13} /> Dev container is ready
              </Match>
              <Match when={state() === "failed"}>
                <CircleAlert size={13} /> Failed
              </Match>
              <Match when={true}>
                <Box size={13} /> Docker
              </Match>
            </Switch>
          </span>
          <button class="btn-secondary" onClick={close}>
            {state() === "building" ? "Stop" : "Cancel"}
          </button>
          <Show when={state() === "idle" || state() === "failed"}>
            <button
              class="btn-primary"
              disabled={!current()?.config}
              onClick={() => void build()}
              data-testid="devcontainer-build"
            >
              {state() === "failed" ? "Retry" : props.request.rebuild ? "Rebuild Container" : "Build Container and Continue"}
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
}
