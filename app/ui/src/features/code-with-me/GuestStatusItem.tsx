/**
 * Status bar widget of a Code With Me guest, its only Code With Me item: "Code With Me: <host>"
 * with the connection path and round trip time to the host ("Direct · 23 ms"), or
 * "Reconnecting…" while a dropped connection is being restored. Clicking it shows the session
 * (host, project, permission, connection, relay) with Leave Session.
 */
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { TriangleAlert, Users } from "lucide-solid";
import type { ConnectionPath } from "@/ipc/collab";
import { collabApi } from "@/ipc/collab";
import { workspace } from "@/state/workspace";
import { showBalloon } from "@/keymap";
import { StatusPopup } from "@/shell/StatusPopup";
import { guestLevel, LEVEL_NAMES } from "./guest";
import { session } from "./store";
import { connectionLabel, isReconnecting } from "./reconnect";
import { connectionOf, pathLabel, pathTitle, relayProblem } from "./connection";
import { RelayLine } from "./ConnectionStatus";
import { tooltip } from "@/shell/tooltip";
import { t } from "@/i18n";

/** Latency class: fast / slow / very slow, colored in the widget. */
export function latencyClass(ms: number | null): "good" | "fair" | "poor" | "unknown" {
  if (ms == null) return "unknown";
  if (ms < 80) return "good";
  if (ms < 250) return "fair";
  return "poor";
}

/** The item's detail: "Direct · 23 ms", "Relay (relay.example.com)", "23 ms" or nothing. */
export function guestStatusDetail(path: ConnectionPath | undefined, ms: number | null): string | null {
  const time = ms != null ? t("tools.codeWithMe.milliseconds", { ms }) : null;
  if (!path) return time;
  return time ? `${pathLabel(path)} · ${time}` : pathLabel(path);
}

export function GuestStatusItem() {
  const [open, setOpen] = createSignal(false);
  const [pinged, setLatency] = createSignal<number | null>(null);
  /** The path to the host (participant 0) while the connection is up. */
  const path = () => (isReconnecting() ? undefined : connectionOf(session(), 0));
  /** The measured round trip, or the one the backend reported for the path. */
  const latency = () => pinged() ?? path()?.rttMs ?? null;
  const detail = () => guestStatusDetail(path(), latency());
  const host = () => workspace()?.collab?.host ?? session().hostLabel ?? t("tools.codeWithMe.hostFallback");
  let el: HTMLDivElement | undefined;
  const measure = () => (isReconnecting() ? setLatency(null) : void collabApi.ping().then(setLatency, () => setLatency(null)));
  const reconnecting = () => (isReconnecting() ? connectionLabel(session().connection) : null);
  onMount(() => {
    measure();
    const timer = setInterval(measure, 5000);
    onCleanup(() => clearInterval(timer));
  });

  const leave = async () => {
    setOpen(false);
    try {
      await collabApi.leave();
    } catch (e) {
      showBalloon(t("tools.codeWithMe.cannotLeaveSession"), String((e as Error)?.message ?? e));
    }
  };

  return (
    <div class="cwm-status-wrap" ref={el}>
      <button
        class="status-item cwm-status"
        data-latency={latencyClass(latency())}
        classList={{ reconnecting: !!reconnecting(), relay: path()?.kind === "relay", problem: !reconnecting() && relayProblem(session()) }}
        data-path={path()?.kind}
        {...tooltip(
          () => {
            const p = path();
            const title = t("tools.codeWithMe.guestOf", { host: workspace()?.collab?.label ?? host() });
            return p ? `${title}\n${pathTitle(p)}` : title;
          },
          { open, label: false },
        )}
        aria-haspopup="dialog"
        aria-expanded={open()}
        onClick={() => setOpen((v) => !v)}
        data-testid="cwm-status"
      >
        <Show when={!reconnecting() && relayProblem(session())} fallback={<Users size={12} />}>
          <TriangleAlert size={12} />
        </Show>
        <span>{t("tools.codeWithMe.titleWith", { value: reconnecting() ? t("tools.codeWithMe.reconnecting") : host() })}</span>
        <Show when={!reconnecting() && detail()}>{(text) => <span class="cwm-status-detail">{text()}</span>}</Show>
      </button>
      <Show when={open()}>
        <StatusPopup anchor={el!} align="end" class="cwm-status-popup" label={t("tools.codeWithMe.title")} onClose={() => setOpen(false)}>
          <div class="cwm-status-title">
            <Users size={14} /> {workspace()?.collab?.label ?? host()}
          </div>
          <dl class="cwm-status-grid">
            <dt>{t("tools.codeWithMe.project")}</dt>
            <dd>{workspace()?.name}</dd>
            <dt>{t("tools.codeWithMe.youAre")}</dt>
            <dd>{t("tools.codeWithMe.aGuestWith", { level: LEVEL_NAMES[guestLevel() ?? "readOnly"] })}</dd>
            <Show when={reconnecting()}>
              {(label) => (
                <>
                  <dt>{t("tools.codeWithMe.connection")}</dt>
                  <dd class="cwm-status-reconnecting">{label()}</dd>
                </>
              )}
            </Show>
            <Show when={path()}>
              {(p) => (
                <>
                  <dt>{t("tools.codeWithMe.connection")}</dt>
                  <dd data-testid="cwm-guest-path" title={pathTitle(p())}>
                    {pathLabel(p())}
                  </dd>
                </>
              )}
            </Show>
            <Show when={latency() != null && !reconnecting()}>
              <dt>{t("tools.codeWithMe.latency")}</dt>
              <dd>{t("tools.codeWithMe.milliseconds", { ms: latency() })}</dd>
            </Show>
            <dt>{t("tools.codeWithMe.services")}</dt>
            <dd>{t("tools.codeWithMe.servicesOnHost")}</dd>
          </dl>
          <Show when={!reconnecting()}>
            <RelayLine />
          </Show>
          <div class="cwm-status-actions">
            <button class="btn-secondary" onClick={() => void leave()} data-action="CodeWithMe.LeaveSession">
              {t("tools.codeWithMe.leaveSession")}
            </button>
          </div>
        </StatusPopup>
      </Show>
    </div>
  );
}
