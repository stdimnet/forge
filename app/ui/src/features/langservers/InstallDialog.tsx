/**
 * "Install <server>": pick how to install it, see the exact command, confirm, then watch
 * the streamed output. Commands that need root (apt, pacman) or a browser are shown for
 * copying only. In a remote project the server is installed on the project's host, where
 * its language servers run.
 */
import { createMemo, createSignal, For, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import { Portal } from "solid-js/web";
import { CircleCheck, CircleX, Copy, ExternalLink, LoaderCircle, TriangleAlert, X } from "lucide-solid";
import { langserversApi, type InstallLine, type InstallOutcome, type ServerRow } from "@/ipc/langservers";
import { workspace } from "@/state/workspace";
import { applyServers, onInstallOutput } from "./store";
import { t } from "@/i18n";

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
  /** `user@host` of a remote project: the install runs there. */
  const host = () => workspace()?.remote?.label;
  const machine = () => host() ?? "this machine";

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
            <span>
              {t("tools.langservers.install", { name: props.row.name })}
              <Show when={host()}>
                {(h) => <span class="ls-dialog-host"> on {h()}</span>}
              </Show>
            </span>
            <button class="icon-btn" title={t("common.close")} onClick={close}>
              <X size={15} />
            </button>
          </header>
          <div class="ls-dialog-body">
            <Show when={phase() === "confirm"}>
              <p class="ls-dialog-text">
                {t("tools.langservers.supportNeeds", { languages: props.row.languages.join(", ") })} <b>{props.row.command}</b>{t("tools.langservers.chooseHowToInstallItLabel")}
              </p>
              <div class="ls-options">
                <For each={options()}>
                  {(o) => (
                    <label class="ls-option" classList={{ selected: choice() === o.index }}>
                      <input type="radio" name="ls-install" checked={choice() === o.index} onChange={() => setChoice(o.index)} />
                      <span class="ls-tool">{o.tool}</span>
                      <span class="ls-option-hint muted">
                        {o.url ? t("tools.langservers.downloadFromTheReleasePage") : !o.runnable ? t("tools.langservers.needsAdministratorRights") : o.available ? "" : t("tools.langservers.notFound", { value: o.command.split(" ")[0] })}
                      </span>
                    </label>
                  )}
                </For>
              </div>
              <div class="ls-command">
                <code>{option()?.command}</code>
                <button class="icon-btn" title={copied() ? t("common.copied") : t("common.copy")} onClick={() => void copy()}>
                  {copied() ? <CircleCheck size={14} /> : <Copy size={14} />}
                </button>
              </div>
              <Switch>
                <Match when={option()?.url}>
                  <p class="ls-hint">{t("tools.langservers.downloadTheServerForYour")}</p>
                </Match>
                <Match when={!option()?.runnable}>
                  <p class="ls-hint">{t("tools.langservers.runThisCommandInA")}</p>
                </Match>
                <Match when={option()?.managed}>
                  <p class="ls-hint">
                    {t("tools.langservers.forgeDownloadsThePackageFrom", { value: host() ? ` on ${host()}` : "" })}
                  </p>
                </Match>
                <Match when={!option()?.available}>
                  <p class="ls-hint ls-warn">
                    <TriangleAlert size={13} /> {t("tools.langservers.wasNotFoundOn", { value: option()?.command.split(" ")[0], machine: machine() })}
                  </p>
                </Match>
                <Match when={true}>
                  <p class="ls-hint">
                    {t("tools.langservers.forgeRunsThisCommandOn", { value: host() ?? t("tools.langservers.yourMachine") })}
                  </p>
                </Match>
              </Switch>
            </Show>
            <Show when={phase() !== "confirm"}>
              <div class="ls-progress-line">
                <Switch>
                  <Match when={phase() === "running"}>
                    <LoaderCircle size={14} class="lsp-spin" />
                    <span>
                      {t("tools.langservers.installingEllipsis", { name: props.row.name, value: host() ? ` on ${host()}` : "" })}
                    </span>
                  </Match>
                  <Match when={succeeded()}>
                    <CircleCheck size={14} class="ls-ok" />
                    <span>{outcomeText(outcome()!, props.row.name)}</span>
                  </Match>
                  <Match when={true}>
                    <CircleX size={14} class="ls-bad" />
                    <span>{error() ?? (outcome() ? outcomeText(outcome()!, props.row.name) : t("tools.langservers.theInstallationFailed"))}</span>
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
              {t("tools.langservers.homepage", { license: props.row.license })} <ExternalLink size={12} />
            </a>
            <span class="ls-spacer" />
            <Switch>
              <Match when={phase() === "confirm"}>
                <button class="btn-secondary" onClick={close}>
                  {t("common.cancel")}
                </button>
                <Show
                  when={option()?.runnable && option()?.available}
                  fallback={
                    <button class="btn-primary" onClick={() => void copy()}>
                      {copied() ? t("common.copied") : t("tools.langservers.copyCommand")}
                    </button>
                  }
                >
                  <button class="btn-primary" onClick={() => void start()}>
                    {t("common.install")}
                  </button>
                </Show>
              </Match>
              <Match when={phase() === "running"}>
                <button class="btn-secondary" onClick={() => void langserversApi.cancelInstall(props.row.id)}>
                  {t("common.stop")}
                </button>
              </Match>
              <Match when={true}>
                <button class="btn-primary" onClick={props.onClose}>
                  {t("common.close")}
                </button>
              </Match>
            </Switch>
          </footer>
        </div>
      </div>
    </Portal>
  );
}
