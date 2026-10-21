/**
 * "Install <server>": pick how to install it, see the exact command, confirm, then watch
 * the streamed output. Commands that need root (apt, pacman) or a browser are shown for
 * copying only.
 */
import { createMemo, createSignal, For, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import { Portal } from "solid-js/web";
import { CircleCheck, CircleX, Copy, ExternalLink, LoaderCircle, TriangleAlert, X } from "lucide-solid";
import { langserversApi, type InstallLine, type InstallOutcome, type ServerRow } from "@/ipc/langservers";
import { applyServers, onInstallOutput } from "./store";
import { INTELEPHENSE_ID } from "./autoInstall";
import { ensureLicence } from "./managed";

type Phase = "confirm" | "running" | "done";

const errorText = (e: unknown) => String((e as Error)?.message ?? e);

export function outcomeText(o: InstallOutcome, name: string): string {
  if (o === "succeeded") return `${name} was installed.`;
  if (o === "cancelled") return "The installation was cancelled.";
  return o.failed.code != null ? `The installation failed (exit code ${o.failed.code}).` : "The installation failed.";
}

export function InstallDialog(props: { row: ServerRow; onClose: () => void }) {
  const options = () => props.row.install;
  const firstRunnable = options().find((o) => o.runnable && o.available) ?? options().find((o) => o.runnable) ?? options()[0];
  const [choice, setChoice] = createSignal(firstRunnable?.index ?? 0);
  const [phase, setPhase] = createSignal<Phase>("confirm");
  const [lines, setLines] = createSignal<InstallLine[]>([]);
  const [outcome, setOutcome] = createSignal<InstallOutcome | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [copied, setCopied] = createSignal(false);
  const option = createMemo(() => options().find((o) => o.index === choice()));
  let log: HTMLDivElement | undefined;

  onMount(() => {
    const off = onInstallOutput((l) => {
      if (l.id !== props.row.id) return;
      setLines((prev) => [...prev, l]);
      queueMicrotask(() => log && (log.scrollTop = log.scrollHeight));
    });
    onCleanup(off);
  });

  const start = async () => {
    const o = option();
    if (!o) return;
    // Intelephense is downloaded only after its licence was accepted (once).
    if (o.managed && props.row.id === INTELEPHENSE_ID && !(await ensureLicence())) return;
    setPhase("running");
    setLines([]);
    setError(null);
    try {
      setOutcome(await langserversApi.install(props.row.id, o.index));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setPhase("done");
      void applyServers();
    }
  };

  const copy = async () => {
    await navigator.clipboard?.writeText(option()?.command ?? "").catch(() => undefined);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  const close = () => {
    if (phase() === "running") void langserversApi.cancelInstall(props.row.id);
    props.onClose();
  };

  const succeeded = () => outcome() === "succeeded";

  return (
    <Portal>
      <div class="ls-overlay" onMouseDown={(e) => e.target === e.currentTarget && phase() !== "running" && close()}>
        <div
          class="ls-dialog ls-install-dialog"
          role="dialog"
          aria-label={`Install ${props.row.name}`}
          onKeyDown={(e) => {
            if (e.key === "Escape" && phase() !== "running") close();
          }}
        >
          <header class="ls-dialog-title">
            <span>Install {props.row.name}</span>
            <button class="icon-btn" title="Close" onClick={close}>
              <X size={15} />
            </button>
          </header>
          <div class="ls-dialog-body">
            <Show when={phase() === "confirm"}>
              <p class="ls-dialog-text">
                {props.row.languages.join(", ")} support needs <b>{props.row.command}</b>. Choose how to install it:
              </p>
              <div class="ls-options">
                <For each={options()}>
                  {(o) => (
                    <label class="ls-option" classList={{ selected: choice() === o.index }}>
                      <input type="radio" name="ls-install" checked={choice() === o.index} onChange={() => setChoice(o.index)} />
                      <span class="ls-tool">{o.tool}</span>
                      <span class="ls-option-hint muted">
                        {o.url ? "download from the release page" : !o.runnable ? "needs administrator rights" : o.available ? "" : `${o.command.split(" ")[0]} not found`}
                      </span>
                    </label>
                  )}
                </For>
              </div>
              <div class="ls-command">
                <code>{option()?.command}</code>
                <button class="icon-btn" title={copied() ? "Copied" : "Copy"} onClick={() => void copy()}>
                  {copied() ? <CircleCheck size={14} /> : <Copy size={14} />}
                </button>
              </div>
              <Switch>
                <Match when={option()?.url}>
                  <p class="ls-hint">Download the server for your platform and set its path in the Language Servers settings.</p>
                </Match>
                <Match when={!option()?.runnable}>
                  <p class="ls-hint">Run this command in a terminal; it needs administrator rights.</p>
                </Match>
                <Match when={option()?.managed}>
                  <p class="ls-hint">
                    Forge downloads the package from the npm registry (checking its sha512 hash) into its own server
                    directory, and Node.js too if none is installed. npm is not needed.
                  </p>
                </Match>
                <Match when={!option()?.available}>
                  <p class="ls-hint ls-warn">
                    <TriangleAlert size={13} /> {option()?.command.split(" ")[0]} was not found on this machine.
                  </p>
                </Match>
                <Match when={true}>
                  <p class="ls-hint">Forge runs this command on your machine and shows its output. This may take a few minutes.</p>
                </Match>
              </Switch>
            </Show>
            <Show when={phase() !== "confirm"}>
              <div class="ls-progress-line">
                <Switch>
                  <Match when={phase() === "running"}>
                    <LoaderCircle size={14} class="lsp-spin" />
                    <span>Installing {props.row.name}…</span>
                  </Match>
                  <Match when={succeeded()}>
                    <CircleCheck size={14} class="ls-ok" />
                    <span>{outcomeText(outcome()!, props.row.name)}</span>
                  </Match>
                  <Match when={true}>
                    <CircleX size={14} class="ls-bad" />
                    <span>{error() ?? (outcome() ? outcomeText(outcome()!, props.row.name) : "The installation failed.")}</span>
                  </Match>
                </Switch>
              </div>
              <div class="ls-log" ref={log} data-testid="install-log">
                <For each={lines()}>
                  {(l) => <div class={`ls-log-line ${l.stream}`}>{l.line || " "}</div>}
                </For>
              </div>
            </Show>
          </div>
          <footer class="ls-dialog-footer">
            <a class="ls-link" href={props.row.homepage} target="_blank" rel="noreferrer noopener">
              {props.row.license} · Homepage <ExternalLink size={12} />
            </a>
            <span class="ls-spacer" />
            <Switch>
              <Match when={phase() === "confirm"}>
                <button class="btn-secondary" onClick={close}>
                  Cancel
                </button>
                <Show
                  when={option()?.runnable && option()?.available}
                  fallback={
                    <button class="btn-primary" onClick={() => void copy()}>
                      {copied() ? "Copied" : "Copy Command"}
                    </button>
                  }
                >
                  <button class="btn-primary" onClick={() => void start()}>
                    Install
                  </button>
                </Show>
              </Match>
              <Match when={phase() === "running"}>
                <button class="btn-secondary" onClick={() => void langserversApi.cancelInstall(props.row.id)}>
                  Stop
                </button>
              </Match>
              <Match when={true}>
                <button class="btn-primary" onClick={props.onClose}>
                  Close
                </button>
              </Match>
            </Switch>
          </footer>
        </div>
      </div>
    </Portal>
  );
}
