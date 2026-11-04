/**
 * Status bar widget of a remote project: the host with the round trip time, "Reconnecting…"
 * while the connection is re-established, or "Disconnected". Clicking it shows the
 * connection details with Reconnect.
 */
import { createSignal, Match, Show, Switch } from "solid-js";
import { LoaderCircle, Server, Unplug } from "lucide-solid";
import { remoteApi, type RemoteStatus } from "@/ipc/remote";
import { workspace } from "@/state/workspace";
import { showBalloon } from "@/keymap";
import { StatusPopup } from "@/shell/StatusPopup";
import { remoteStatus } from "./store";
import { t } from "@/i18n";

/** Latency class: fast / slow / very slow, colored in the widget. */
export function latencyLevel(ms: number | null | undefined): "good" | "fair" | "poor" | "unknown" {
  if (ms == null) return "unknown";
  if (ms < 80) return "good";
  if (ms < 250) return "fair";
  return "poor";
}

export function statusText(s: RemoteStatus): string {
  switch (s.state) {
    case "connected":
      return s.latencyMs != null ? `${s.latencyMs} ms` : "Connected";
    case "connecting":
      return s.message;
    case "reconnecting":
      return s.attempt > 1 ? `Reconnecting… (attempt ${s.attempt})` : "Reconnecting…";
    case "disconnected":
      return "Disconnected";
  }
}

export function RemoteStatusItem() {
  const [open, setOpen] = createSignal(false);
  const label = () => remoteStatus()?.label ?? workspace()?.remote?.label ?? "";
  let el: HTMLDivElement | undefined;

  const reconnect = async () => {
    setOpen(false);
    try {
      await remoteApi.reconnect();
    } catch (e) {
      showBalloon(t("tools.remote.cannotReconnect"), String((e as Error)?.message ?? e));
    }
  };

  return (
    <div class="remote-status-wrap" ref={el}>
      <button
        class="status-item remote-status"
        data-state={remoteStatus()?.state ?? "connected"}
        data-latency={latencyLevel(remoteStatus()?.state === "connected" ? (remoteStatus() as { latencyMs?: number }).latencyMs : null)}
        title={workspace()?.remote?.kind ? label() : t("tools.remote.remoteHost", { label: label() })}
        onClick={() => setOpen((v) => !v)}
        data-testid="remote-status"
      >
        <Switch fallback={<Server size={12} />}>
          <Match when={remoteStatus()?.state === "reconnecting" || remoteStatus()?.state === "connecting"}>
            <LoaderCircle size={12} class="ssh-spin" />
          </Match>
          <Match when={remoteStatus()?.state === "disconnected"}>
            <Unplug size={12} />
          </Match>
        </Switch>
        <span>{label()}</span>
        <Show when={remoteStatus()}>{(s) => <span class="remote-status-detail">{statusText(s())}</span>}</Show>
      </button>
      <Show when={open()}>
        <StatusPopup anchor={el!} class="remote-popup" label={t("tools.remote.remoteHost2")} onClose={() => setOpen(false)}>
          <div class="remote-popup-title">
            <Server size={14} /> {label()}
          </div>
          <dl class="remote-popup-grid">
            <dt>{t("common.project")}</dt>
            <dd class="mono">{workspace()?.remote?.path}</dd>
            <dt>{t("common.status")}</dt>
            <dd>
              <Show when={remoteStatus()} fallback="Connected">
                {(s) => (
                  <Switch>
                    <Match when={s().state === "connected"}>{t("tools.remote.connected")}</Match>
                    <Match when={s().state === "reconnecting" && (s() as Extract<RemoteStatus, { state: "reconnecting" }>)}>
                      {(r) => `Reconnecting (attempt ${r().attempt}, next try in ${r().nextRetrySecs} s): ${r().error}`}
                    </Match>
                    <Match when={s().state === "disconnected" && (s() as Extract<RemoteStatus, { state: "disconnected" }>)}>
                      {(d) => `Disconnected${d().error ? `: ${d().error}` : ""}`}
                    </Match>
                    <Match when={s().state === "connecting"}>{statusText(s())}</Match>
                  </Switch>
                )}
              </Show>
            </dd>
            <Show when={remoteStatus()?.state === "connected"}>
              <dt>{t("tools.remote.latency")}</dt>
              <dd>{statusText(remoteStatus()!)}</dd>
            </Show>
            <dt>{t("tools.remote.services")}</dt>
            <dd>
              {t("tools.remote.indexingLanguageServersGitAnd", { value: workspace()?.remote?.kind?.type === "devContainer"
                ? t("tools.remote.inTheContainer")
                : workspace()?.remote?.kind?.type === "wsl"
                  ? t("tools.remote.inTheDistribution")
                  : t("tools.remote.onTheHost") })}
            </dd>
          </dl>
          <div class="remote-popup-actions">
            <button class="btn-secondary" onClick={() => void reconnect()}>
              {t("tools.remote.reconnect")}
            </button>
          </div>
        </StatusPopup>
      </Show>
    </div>
  );
}
