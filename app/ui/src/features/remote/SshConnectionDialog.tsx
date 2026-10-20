/**
 * New SSH Connection (Welcome screen > Remote Development, like JetBrains Gateway):
 * 1. Connect to SSH: a saved configuration or new connection fields, Test Connection, the
 *    host key question; "Check Connection and Continue" starts the Forge agent on the host.
 * 2. Choose Project: the project folder on the host (typed or picked in the remote folder
 *    chooser); "Open Project" opens it in the IDE.
 */
import { createEffect, createResource, createSignal, For, on, onCleanup, Show } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { ChevronRight, Folder, LoaderCircle, Server, X } from "lucide-solid";
import { connectErrorText, remoteApi, type ConnectError, type SshConfiguration } from "@/ipc/remote";
import { openRemoteProject } from "@/state/workspace";
import { SshForm } from "./SshForm";
import { TestResultLine, useConnectionTest } from "./TestConnection";
import {
  blankConfig,
  chooseRemoteFolder,
  connectWithConfirmation,
  connecting,
  setConnecting,
  setSshDialog,
  sshDialog,
  type SshDialogRequest,
} from "./store";

const NEW = "__new__";

interface Connected {
  connectionId: string;
  label: string;
  home: string;
  hostname: string;
  system: string;
}

export function SshConnectionDialog() {
  return <Show when={sshDialog()}>{(req) => <DialogBody request={req()} />}</Show>;
}

function DialogBody(props: { request: SshDialogRequest }) {
  const [saved] = createResource(() => remoteApi.configs().catch(() => []));
  const [config, setConfig] = createStore<SshConfiguration>(structuredClone(props.request.config));
  const [selected, setSelected] = createSignal(props.request.config.id || NEW);
  const [secret, setSecret] = createSignal("");
  const [problems, setProblems] = createSignal<string[]>([]);
  const [error, setError] = createSignal<ConnectError | string | null>(props.request.error ?? null);
  const [step, setStep] = createSignal<1 | 2>(1);
  const [connected, setConnected] = createSignal<Connected | null>(null);
  const [path, setPath] = createSignal(props.request.path ?? "");
  const [opening, setOpening] = createSignal(false);
  const test = useConnectionTest();

  const selectedConfig = () => saved()?.find((c) => c.id === selected());
  const label = () => {
    const host = config.host.trim() || "host";
    const user = config.user.trim();
    return `${user ? `${user}@` : ""}${host}${config.port && config.port !== 22 ? `:${config.port}` : ""}`;
  };

  const update = (patch: Partial<SshConfiguration>) => {
    setConfig(patch);
    test.reset();
    setError(null);
  };

  // Live validation, computed by the backend (Rust) like everything else.
  createEffect(
    on(
      () => JSON.stringify(config),
      () => {
        const snapshot = structuredClone({ ...config });
        void remoteApi.validate(snapshot).then(setProblems).catch(() => setProblems([]));
      },
    ),
  );

  const pick = (id: string) => {
    setSelected(id);
    const c = saved()?.find((x) => x.id === id);
    setConfig(reconcile(c ? { ...structuredClone(c) } : blankConfig()));
    setSecret("");
    test.reset();
    setError(null);
  };

  /** Connection to drop when the dialog closes without opening a project. */
  let pending: string | null = null;
  const close = () => {
    if (pending) void remoteApi.disconnect(pending);
    pending = null;
    setSshDialog(null);
  };
  onCleanup(() => {
    if (pending) void remoteApi.disconnect(pending);
  });

  const busy = () => !!connecting() || opening();

  const continueToProject = async () => {
    if (problems().length) return;
    setError(null);
    setConnecting({ label: label(), message: `Connecting to ${label()}…` });
    try {
      // Save new or changed connections so the project can reconnect later.
      const toSave = { ...config };
      const view = await remoteApi.saveConfig(toSave, secret() || undefined);
      setConfig("id", view.id);
      const result = await connectWithConfirmation({ ...config, id: view.id }, secret() || undefined);
      if ("error" in result) {
        setError(result.error);
        return;
      }
      pending = result.connectionId;
      setConnected(result);
      if (!path()) setPath(result.home);
      setStep(2);
    } catch (e) {
      setError(String((e as Error)?.message ?? e));
    } finally {
      setConnecting(null);
    }
  };

  const browse = async () => {
    const c = connected();
    if (!c) return;
    const chosen = await chooseRemoteFolder(c.connectionId, c.label, path() || c.home);
    if (chosen) setPath(chosen);
  };

  const open = async () => {
    const c = connected();
    if (!c || !path().trim()) return;
    setOpening(true);
    setError(null);
    try {
      await openRemoteProject(c.connectionId, path().trim());
      pending = null;
      setSshDialog(null);
    } catch (e) {
      setError(String((e as Error)?.message ?? e));
    } finally {
      setOpening(false);
    }
  };

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape" && !busy()) close();
  };

  return (
    <div class="ssh-overlay" onMouseDown={(e) => e.target === e.currentTarget && !busy() && close()}>
      <div class="ssh-dialog" role="dialog" aria-label="New SSH Connection" onKeyDown={onKey} data-testid="ssh-dialog">
        <div class="ssh-titlebar">
          <span>New SSH Connection</span>
          <button class="icon-btn" title="Close" onClick={close} disabled={busy()}>
            <X size={15} />
          </button>
        </div>

        <div class="ssh-steps">
          <span class="ssh-step" classList={{ active: step() === 1, done: step() === 2 }}>
            <span class="ssh-step-no">1</span> Connect to SSH
          </span>
          <ChevronRight size={14} class="muted" />
          <span class="ssh-step" classList={{ active: step() === 2 }}>
            <span class="ssh-step-no">2</span> Choose Project
          </span>
        </div>

        <div class="ssh-body">
          <Show when={step() === 1}>
            <div class="ssh-config-row">
              <label class="ssh-label" for="ssh-config-select">
                SSH configuration:
              </label>
              <select
                id="ssh-config-select"
                class="ssh-input"
                value={selected()}
                onChange={(e) => pick(e.currentTarget.value)}
                disabled={busy()}
              >
                <option value={NEW}>New connection</option>
                <For each={saved() ?? []}>{(c) => <option value={c.id}>{c.name || c.label}</option>}</For>
              </select>
            </div>
            <SshForm
              config={config}
              update={update}
              secret={secret()}
              setSecret={(s) => {
                setSecret(s);
                test.reset();
              }}
              hasSecret={selectedConfig()?.hasSecret}
              problems={config.host || config.user ? problems() : []}
              autofocus
            />
            <div class="ssh-test-row">
              <button
                class="btn-secondary"
                disabled={busy() || problems().length > 0 || test.state()?.running === true}
                onClick={() => void test.run({ ...config }, secret())}
                data-testid="ssh-test"
              >
                Test Connection
              </button>
              <TestResultLine state={test.state()} host={config.host.trim()} />
            </div>
          </Show>

          <Show when={step() === 2 && connected()}>
            {(c) => (
              <div class="ssh-project-step">
                <div class="ssh-connected">
                  <Server size={18} class="ssh-ok" />
                  <div>
                    <div class="ssh-connected-title">Connected to {c().label}</div>
                    <div class="muted ssh-connected-sub">
                      {c().hostname} · {c().system} · Forge agent running
                    </div>
                  </div>
                </div>
                <label class="ssh-label" for="ssh-project-path">
                  Project directory:
                </label>
                <div class="ssh-path">
                  <input
                    id="ssh-project-path"
                    class="ssh-input mono"
                    value={path()}
                    spellcheck={false}
                    onInput={(e) => setPath(e.currentTarget.value)}
                    onKeyDown={(e) => e.key === "Enter" && void open()}
                    data-testid="ssh-project-path"
                  />
                  <button class="ssh-browse" title="Browse…" onClick={() => void browse()} data-testid="ssh-browse">
                    <Folder size={14} />
                  </button>
                </div>
                <p class="ssh-hint">
                  Forge installs its agent in <code>~/.cache/forge-agent</code> on the host. Indexing, Intelephense and
                  other language servers, git, terminals and run configurations work on the host; files open in this
                  window.
                </p>
              </div>
            )}
          </Show>

          <Show when={error()}>
            {(e) => (
              <div class="ssh-error" data-testid="ssh-error">
                {typeof e() === "string" ? (e() as string) : connectErrorText(e() as ConnectError)}
              </div>
            )}
          </Show>
        </div>

        <div class="ssh-footer">
          <div class="ssh-progress">
            <Show when={connecting()}>
              {(c) => (
                <>
                  <LoaderCircle size={14} class="ssh-spin" />
                  <span>{c().message}</span>
                </>
              )}
            </Show>
            <Show when={opening()}>
              <LoaderCircle size={14} class="ssh-spin" />
              <span>Opening {path()}…</span>
            </Show>
          </div>
          <Show when={step() === 2}>
            <button class="btn-secondary" disabled={busy()} onClick={() => setStep(1)}>
              Back
            </button>
          </Show>
          <button class="btn-secondary" disabled={busy()} onClick={close}>
            Cancel
          </button>
          <Show
            when={step() === 2}
            fallback={
              <button
                class="btn-primary"
                disabled={busy() || problems().length > 0}
                onClick={() => void continueToProject()}
                data-testid="ssh-continue"
              >
                Check Connection and Continue
              </button>
            }
          >
            <button class="btn-primary" disabled={busy() || !path().trim()} onClick={() => void open()} data-testid="ssh-open">
              Open Project
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
}
