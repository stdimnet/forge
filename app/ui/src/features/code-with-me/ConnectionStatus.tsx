/**
 * How the Code With Me connections travel: a badge per connection (Direct / Relay) in the
 * participant rows of the popup, the relay state and the status bar item.
 */
import { Show } from "solid-js";
import { Network, TriangleAlert, Waypoints } from "lucide-solid";
import type { ConnectionPath, Participant } from "@/ipc/collab";
import { connectionOf, pathLabel, pathTitle, relayProblem, relaySummary, statusBarText, statusBarTitle } from "./connection";
import { popupOpen, session, setPopupOpen } from "./store";
import { tooltip } from "@/shell/tooltip";
import { isReconnecting } from "./reconnect";
import { t } from "@/i18n";

/** "Direct" or "Relay (relay.example.com)" with an icon and the round-trip time. */
export function PathBadge(props: { path: ConnectionPath }) {
  return (
    <span class="cwm-path" classList={{ relay: props.path.kind === "relay" }} data-path={props.path.kind} title={pathTitle(props.path)}>
      {props.path.kind === "direct" ? <Network size={12} /> : <Waypoints size={12} />}
      {pathLabel(props.path)}
      <Show when={props.path.rttMs !== undefined}>
        <span class="cwm-path-rtt">{t("tools.codeWithMe.milliseconds", { ms: props.path.rttMs })}</span>
      </Show>
    </span>
  );
}

/**
 * The connection of a participant row in the session popup: the path badge, "Connecting…" or
 * "Reconnecting…". Hosts see it for every guest, guests for the host (guests are not
 * connected to each other).
 */
export function ParticipantPath(props: { participant: Participant }) {
  const p = () => props.participant;
  const shown = () => p().id !== session().me && (session().role === "host" || p().role === "host");
  const reconnecting = () => !!p().reconnecting || (p().role === "host" && isReconnecting());
  return (
    <Show when={shown()}>
      <Show
        when={!reconnecting() && connectionOf(session(), p().id)}
        fallback={
          reconnecting() ? (
            <span class="cwm-path pending cwm-participant-reconnecting" title={t("tools.codeWithMe.connectionDropped")}>
              {t("tools.codeWithMe.reconnecting")}
            </span>
          ) : (
            <span class="cwm-path pending muted">{t("tools.codeWithMe.connecting")}</span>
          )
        }
      >
        {(path) => <PathBadge path={path()} />}
      </Show>
    </Show>
  );
}

/** The state of the relay server below the participants, when a relay is set. */
export function RelayLine() {
  return (
    <Show when={relaySummary(session())}>
      {(text) => (
        <div class="cwm-relay-line" classList={{ problem: relayProblem(session()) }} data-testid="cwm-relay-status">
          <Show when={relayProblem(session())} fallback={<Waypoints size={12} />}>
            <TriangleAlert size={12} />
          </Show>
          {text()}
        </div>
      )}
    </Show>
  );
}

/** Status bar item of the host: the path in use, or how many guests are relayed (guests show theirs in `GuestStatusItem`). */
export function ConnectionStatusItem() {
  const first = () => session().connections?.[0];
  return (
    <button
      class="status-item cwm-status"
      classList={{ relay: first()?.kind === "relay", problem: relayProblem(session()) }}
      data-status="code-with-me"
      {...tooltip(() => statusBarTitle(session()), { open: popupOpen, label: false })}
      aria-haspopup="dialog"
      aria-expanded={popupOpen()}
      onClick={() => setPopupOpen((o) => !o)}
    >
      <Show when={relayProblem(session())} fallback={first()?.kind === "relay" ? <Waypoints size={12} /> : <Network size={12} />}>
        <TriangleAlert size={12} />
      </Show>
      {t("tools.codeWithMe.titleWith", { value: statusBarText(session()) })}
    </button>
  );
}

