/**
 * How the Code With Me connections travel: a badge per connection (Direct / Relay), the
 * participants block of the popup and the status bar item.
 */
import { For, Show } from "solid-js";
import { Network, TriangleAlert, Waypoints } from "lucide-solid";
import type { ConnectionPath } from "@/ipc/collab";
import { connectionOf, pathLabel, pathTitle, relayProblem, relaySummary, statusBarText, statusBarTitle } from "./connection";
import { session, setPopupOpen } from "./store";
import { isReconnecting } from "./reconnect";

/** "Direct" or "Relay (relay.example.com)" with an icon and the round-trip time. */
export function PathBadge(props: { path: ConnectionPath }) {
  return (
    <span class="cwm-path" classList={{ relay: props.path.kind === "relay" }} data-path={props.path.kind} title={pathTitle(props.path)}>
      {props.path.kind === "direct" ? <Network size={12} /> : <Waypoints size={12} />}
      {pathLabel(props.path)}
      <Show when={props.path.rttMs !== undefined}>
        <span class="cwm-path-rtt">{props.path.rttMs} ms</span>
      </Show>
    </span>
  );
}

/** The participants of the session with the path of each connection, and the relay state. */
export function SessionConnections() {
  const peers = () => session().participants.filter((p) => p.id !== session().me);
  return (
    <div class="cwm-connections" data-testid="cwm-connections">
      <div class="cwm-popup-label muted">Participants</div>
      <For each={peers()}>
        {(p) => (
          <div class="cwm-participant" data-participant={p.id}>
            <span class="cwm-dot" style={{ background: `var(--collab-${p.color})` }} />
            <span class="cwm-participant-name">{p.name}</span>
            <span class="cwm-participant-role muted">{p.role === "host" ? "Host" : "Guest"}</span>
            <Show
              when={!p.reconnecting && !(p.role === "host" && isReconnecting()) && connectionOf(session(), p.id)}
              fallback={
                p.reconnecting || (p.role === "host" && isReconnecting()) ? (
                  <span class="cwm-path pending cwm-participant-reconnecting">Reconnecting…</span>
                ) : (
                  <span class="cwm-path pending muted">Connecting…</span>
                )
              }
            >
              {(path) => <PathBadge path={path()} />}
            </Show>
          </div>
        )}
      </For>
      <Show when={peers().length === 0}>
        <div class="cwm-popup-note muted">No guests yet. Send them the session link.</div>
      </Show>
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
    </div>
  );
}

/** Status bar item while a session is running: the path in use, or how many guests are relayed. */
export function ConnectionStatusItem() {
  const first = () => (session().role === "guest" ? connectionOf(session(), 0) : session().connections?.[0]);
  return (
    <button
      class="status-item cwm-status"
      classList={{ relay: first()?.kind === "relay", problem: relayProblem(session()) }}
      data-status="code-with-me"
      title={statusBarTitle(session())}
      onClick={() => setPopupOpen(true)}
    >
      <Show when={relayProblem(session())} fallback={first()?.kind === "relay" ? <Waypoints size={12} /> : <Network size={12} />}>
        <TriangleAlert size={12} />
      </Show>
      Code With Me: {statusBarText(session())}
    </button>
  );
}

