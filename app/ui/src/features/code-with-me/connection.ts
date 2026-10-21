/**
 * How Code With Me connections travel, in words: direct (LAN, port forward, VPN or a punched
 * hole) or through a relay server. The backend reports the path of every connection
 * (`SessionView.connections`) and the state of the relay (`SessionView.relay`).
 */
import type { ConnectionPath, SessionView } from "@/ipc/collab";

/** `relay.example.com:8443` from `https://relay.example.com:8443/`. */
export function relayHost(url: string): string {
  const rest = url.includes("://") ? url.slice(url.indexOf("://") + 3) : url;
  return rest.split("/")[0];
}

/** "Direct" or "Relay (relay.example.com)". */
export function pathLabel(p: ConnectionPath): string {
  return p.kind === "direct" ? "Direct" : `Relay (${relayHost(p.remote)})`;
}

/** Tooltip text for a connection. */
export function pathTitle(p: ConnectionPath): string {
  const rtt = p.rttMs !== undefined ? `\nRound-trip time: ${p.rttMs} ms` : "";
  if (p.kind === "direct") return `Direct connection to ${p.remote}.${rtt}`;
  return (
    `Through the relay server ${relayHost(p.remote)}. Everything stays end-to-end encrypted; ` +
    `Forge keeps trying to connect directly.${rtt}`
  );
}

/** The connection of one participant (guests: the host is participant 0). */
export function connectionOf(s: SessionView, participant: number): ConnectionPath | undefined {
  return s.connections?.find((c) => c.participant === participant);
}

/** The state of the relay as a sentence, or undefined without a relay. */
export function relaySummary(s: SessionView): string | undefined {
  if (!s.relay) return undefined;
  const host = relayHost(s.relay.url);
  if (s.relay.connected) return `Relay server ${host} connected`;
  return s.relay.error ? `Relay server ${host}: ${s.relay.error}` : `Relay server ${host} not connected`;
}

/** Text of the status bar item, e.g. "Direct · 12 ms" or "2 guests, 1 via relay". */
export function statusBarText(s: SessionView): string {
  const rtt = (p: ConnectionPath) => (p.rttMs !== undefined ? ` · ${p.rttMs} ms` : "");
  if (s.role === "guest") {
    const p = connectionOf(s, 0);
    return p ? `${pathLabel(p)}${rtt(p)}` : "Connecting…";
  }
  const guests = s.participants.filter((p) => p.role === "guest").length;
  if (guests === 0) return s.relay && !s.relay.connected ? "Relay not connected" : "Waiting for guests";
  const relayed = (s.connections ?? []).filter((c) => c.kind === "relay").length;
  const n = `${guests} ${guests === 1 ? "guest" : "guests"}`;
  if (relayed === 0) return `${n}, direct`;
  return relayed === guests ? `${n}, via relay` : `${n}, ${relayed} via relay`;
}

/** Multi-line tooltip of the status bar item. */
export function statusBarTitle(s: SessionView): string {
  const lines = ["Code With Me"];
  const relay = relaySummary(s);
  for (const c of s.connections ?? []) {
    const who = s.participants.find((p) => p.id === c.participant)?.name ?? "Participant";
    lines.push(`${who}: ${pathLabel(c)}${c.rttMs !== undefined ? `, ${c.rttMs} ms` : ""}`);
  }
  if (relay) lines.push(relay);
  return lines.join("\n");
}

/** Whether the relay is configured but unusable (shown as a warning). */
export const relayProblem = (s: SessionView): boolean => !!s.relay && !s.relay.connected;
