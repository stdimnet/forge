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
import { guestLevel, LEVEL_NAMES } from "./guest";
import { session } from "./store";
import { connectionLabel, isReconnecting } from "./reconnect";

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
  const host = () => workspace()?.collab?.host ?? session().hostLabel ?? "host";
  let el: HTMLDivElement | undefined;
  const onDoc = (e: MouseEvent) => {
    if (el && !el.contains(e.target as Node)) setOpen(false);
  };
  const measure = () => (isReconnecting() ? setLatency(null) : void collabApi.ping().then(setLatency, () => setLatency(null)));
  const reconnecting = () => (isReconnecting() ? connectionLabel(session().connection) : null);
  onMount(() => {
    document.addEventListener("mousedown", onDoc);
    measure();
    const timer = setInterval(measure, 5000);
    onCleanup(() => clearInterval(timer));
  });
  onCleanup(() => document.removeEventListener("mousedown", onDoc));

  const leave = async () => {
    setOpen(false);
    try {
      await collabApi.leave();
    } catch (e) {
      showBalloon("Cannot leave the session", String((e as Error)?.message ?? e));
    }
  };

  return (
    <div class="cwm-status-wrap" ref={el}>
      <button
        class="status-item cwm-status"
        data-latency={latencyClass(latency())}
        classList={{ reconnecting: !!reconnecting() }}
        title={`Code With Me: you are a guest of ${workspace()?.collab?.label ?? host()}`}
        aria-haspopup="dialog"
        aria-expanded={open()}
        onClick={() => setOpen((v) => !v)}
        data-testid="cwm-status"
      >
        <Users size={12} />
        <span>Code With Me: {reconnecting() ? "Reconnecting…" : host()}</span>
        <Show when={latency() != null && !reconnecting()}>
          <span class="cwm-status-detail">{latency()} ms</span>
        </Show>
      </button>
      <Show when={open()}>
        <div class="cwm-status-popup" role="dialog" aria-label="Code With Me">
          <div class="cwm-status-title">
            <Users size={14} /> {workspace()?.collab?.label ?? host()}
          </div>
          <dl class="cwm-status-grid">
            <dt>Project</dt>
            <dd>{workspace()?.name}</dd>
            <dt>You are</dt>
            <dd>
              a guest ({LEVEL_NAMES[guestLevel() ?? "readOnly"]})
            </dd>
            <Show when={reconnecting()}>
              {(label) => (
                <>
                  <dt>Connection</dt>
                  <dd class="cwm-status-reconnecting">{label()}</dd>
                </>
              )}
            </Show>
            <Show when={latency() != null && !reconnecting()}>
              <dt>Latency</dt>
              <dd>{latency()} ms</dd>
            </Show>
            <dt>Services</dt>
            <dd>Search, git, language servers and the PHP index run on the host</dd>
          </dl>
          <div class="cwm-status-actions">
            <button class="btn-secondary" onClick={() => void leave()} data-action="CodeWithMe.LeaveSession">
              Leave Session
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
}
