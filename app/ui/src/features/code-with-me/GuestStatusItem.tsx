/**
 * Status bar widget of a Code With Me guest: "Code With Me: <host>" with the round trip time
 * to the host, or "Reconnecting…" while a dropped connection is being restored. Clicking it
 * shows the session (host, project, permission) with Leave Session.
 */
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { Users } from "lucide-solid";
import { collabApi } from "@/ipc/collab";
import { workspace } from "@/state/workspace";
import { showBalloon } from "@/keymap";
import { StatusPopup } from "@/shell/StatusPopup";
import { guestLevel, LEVEL_NAMES } from "./guest";
import { session } from "./store";
import { connectionLabel, isReconnecting } from "./reconnect";
import { t } from "@/i18n";

/** Latency class: fast / slow / very slow, colored in the widget. */
export function latencyClass(ms: number | null): "good" | "fair" | "poor" | "unknown" {
  if (ms == null) return "unknown";
  if (ms < 80) return "good";
  if (ms < 250) return "fair";
  return "poor";
}

export function GuestStatusItem() {
  const [open, setOpen] = createSignal(false);
  const [latency, setLatency] = createSignal<number | null>(null);
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
        classList={{ reconnecting: !!reconnecting() }}
        title={t("tools.codeWithMe.guestOf", { host: workspace()?.collab?.label ?? host() })}
        aria-haspopup="dialog"
        aria-expanded={open()}
        onClick={() => setOpen((v) => !v)}
        data-testid="cwm-status"
      >
        <Users size={12} />
        <span>{t("tools.codeWithMe.titleWith", { value: reconnecting() ? t("tools.codeWithMe.reconnecting") : host() })}</span>
        <Show when={latency() != null && !reconnecting()}>
          <span class="cwm-status-detail">{t("tools.codeWithMe.milliseconds", { ms: latency() })}</span>
        </Show>
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
            <Show when={latency() != null && !reconnecting()}>
              <dt>{t("tools.codeWithMe.latency")}</dt>
              <dd>{t("tools.codeWithMe.milliseconds", { ms: latency() })}</dd>
            </Show>
            <dt>{t("tools.codeWithMe.services")}</dt>
            <dd>{t("tools.codeWithMe.servicesOnHost")}</dd>
          </dl>
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
