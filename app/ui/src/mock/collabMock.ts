/**
 * Browser-mode Code With Me. Nothing leaves the page: there is no network in browser mode.
 */
import { emitMock, registerMock } from "@/ipc";
import { STATUS_EVENT, type RelayStatus, type RelayTokenStatus, type SessionView } from "@/ipc/collab";

const idle = (): SessionView => ({
  role: "none",
  canHost: false,
  hostUnavailable: "Sharing sessions is not available yet.",
  participants: [],
  files: [],
});

registerMock("collab_status", () => idle());

registerMock("collab_parse_link", ({ link }) => {
  const text = String(link ?? "").trim();
  if (!text.startsWith("forge://cwm/join?") && !text.startsWith("cwm1:")) throw new Error("Invalid session link: not a Code With Me link");
  const name = decodeURIComponent(/[?&:]name=([^&#]*)/.exec(text)?.[1] ?? "");
  if (!name) throw new Error("Invalid session link: name is missing");
  return { hostName: name, hostLabel: name, sessionId: /[?&]s=([^&#]*)/.exec(text)?.[1] ?? "" };
});

// ---- relay (Settings | Tools | Code With Me | Relay Server) ------------------------------

let token = false;
const tokenStatus = (): RelayTokenStatus => ({ set: token });

registerMock("collab_relay_token_status", () => tokenStatus());

registerMock("collab_set_relay_token", ({ token: raw }) => {
  const value = String(raw ?? "").trim();
  if (value && !/^[\x21-\x7e]+$/.test(value)) throw new Error("Invalid relay server: the relay token must be printable ASCII without spaces");
  token = value !== "";
  return tokenStatus();
});

registerMock("collab_test_relay", ({ url, public: publicRelays }) => {
  const text = String(url ?? "").trim();
  if (!text && !publicRelays) throw new Error("Enter a relay server first.");
  if (!text) return { url: "https://relay.iroh.example/", connected: true } satisfies RelayStatus;
  const full = text.includes("://") ? text : `https://${text}`;
  if (!/^https?:\/\/[^\s/?#@]+(\/.*)?$/.test(full)) throw new Error("Invalid relay server: the URL has no host name");
  if (full.includes("unreachable")) throw new Error("Cannot reach the relay server (the relay did not answer)");
  return { url: full.replace(/\/+$/, ""), connected: true } satisfies RelayStatus;
});

// Screenshot and test hook: show a session in the popup and the status bar.
declare global {
  interface Window {
    __forgeCollabMock?: { show: (view: SessionView) => void };
  }
}
window.__forgeCollabMock = { show: (view) => emitMock(STATUS_EVENT, view) };
