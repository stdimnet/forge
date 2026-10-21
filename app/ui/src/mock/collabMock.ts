/**
 * Browser-mode Code With Me. Nothing leaves the page: there is no network in browser mode.
 */
import { registerMock } from "@/ipc";
import type { SessionView } from "@/ipc/collab";

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
