/**
 * How Code With Me connections travel, in words: direct (LAN, port forward, VPN or a punched
 * hole) or through a relay server. The backend reports the path of every connection
 * (`SessionView.connections`) and the state of the relay (`SessionView.relay`).
 */
import type { ConnectionPath, SessionView } from "@/ipc/collab";
import { t } from "@/i18n";

/** `relay.example.com:8443` from `https://relay.example.com:8443/`. */
export function relayHost(url: string): string {
  const rest = url.includes("://") ? url.slice(url.indexOf("://") + 3) : url;
  return rest.split("/")[0];
}

/** "Direct" or "Relay (relay.example.com)". */
export function pathLabel(p: ConnectionPath): string {
  return p.kind === "direct" ? t("tools.codeWithMe.direct") : t("tools.codeWithMe.relayPath", { host: relayHost(p.remote) });
}

/** Tooltip text for a connection. */
export function pathTitle(p: ConnectionPath): string {
  const rtt = p.rttMs !== undefined ? `\n${t("tools.codeWithMe.roundTripTime", { ms: p.rttMs })}` : "";
  if (p.kind === "direct") return `${t("tools.codeWithMe.directConnectionTo", { remote: p.remote })}${rtt}`;
  return `${t("tools.codeWithMe.throughRelay", { host: relayHost(p.remote) })}${rtt}`;
}

/** The connection of one participant (guests: the host is participant 0). */
export function connectionOf(s: SessionView, participant: number): ConnectionPath | undefined {
  return s.connections?.find((c) => c.participant === participant);
}

/** The state of the relay as a sentence, or undefined without a relay. */
export function relaySummary(s: SessionView): string | undefined {
  if (!s.relay) return undefined;
  const host = relayHost(s.relay.url);
  if (s.relay.connected) return t("tools.codeWithMe.relayConnected", { host });
  return s.relay.error
    ? t("tools.codeWithMe.relayError", { host, error: s.relay.error })
    : t("tools.codeWithMe.relayNotConnected", { host });
}

/** Text of the status bar item, e.g. "Direct · 12 ms" or "2 guests, 1 via relay". */
export function statusBarText(s: SessionView): string {
  if (s.role === "guest") {
    const p = connectionOf(s, 0);
    if (!p) return t("tools.codeWithMe.connecting");
    return p.rttMs !== undefined ? `${pathLabel(p)} · ${t("tools.codeWithMe.milliseconds", { ms: p.rttMs })}` : pathLabel(p);
  }
  const guests = s.participants.filter((p) => p.role === "guest").length;
  if (guests === 0) return s.relay && !s.relay.connected ? t("tools.codeWithMe.relayNotConnectedShort") : t("tools.codeWithMe.waitingForGuests");
  const relayed = (s.connections ?? []).filter((c) => c.kind === "relay").length;
  if (relayed === 0) return t("tools.codeWithMe.guestsDirect", { count: guests });
  return relayed === guests
    ? t("tools.codeWithMe.guestsViaRelay", { count: guests })
    : t("tools.codeWithMe.guestsPartlyViaRelay", { count: guests, relayed });
}

/** Multi-line tooltip of the status bar item. */
export function statusBarTitle(s: SessionView): string {
  const lines = [t("tools.codeWithMe.title")];
  const relay = relaySummary(s);
  for (const c of s.connections ?? []) {
    const who = s.participants.find((p) => p.id === c.participant)?.name ?? t("tools.codeWithMe.participant");
    lines.push(
      c.rttMs !== undefined
        ? t("tools.codeWithMe.connectionLineRtt", { who, path: pathLabel(c), ms: c.rttMs })
        : t("tools.codeWithMe.connectionLine", { who, path: pathLabel(c) }),
    );
  }
  if (relay) lines.push(relay);
  return lines.join("\n");
}

/** Whether the relay is configured but unusable (shown as a warning). */
export const relayProblem = (s: SessionView): boolean => !!s.relay && !s.relay.connected;
