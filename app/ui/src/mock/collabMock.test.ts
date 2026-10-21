import { describe, expect, it } from "vitest";
import { invoke, listen } from "@/ipc";
import { STATUS_EVENT, type RelayStatus, type RelayTokenStatus, type SessionView } from "@/ipc/collab";
import "./collabMock";

describe("Code With Me mock: relay", () => {
  it("keeps only whether a token is set", async () => {
    expect((await invoke<RelayTokenStatus>("collab_relay_token_status")).set).toBe(false);
    await expect(invoke("collab_set_relay_token", { token: "has space" })).rejects.toThrow(/printable ASCII/);
    const set = await invoke<RelayTokenStatus>("collab_set_relay_token", { token: " team-token " });
    expect(set).toEqual({ set: true });
    expect(JSON.stringify(set)).not.toContain("team-token");
    expect((await invoke<RelayTokenStatus>("collab_set_relay_token", { token: null })).set).toBe(false);
  });

  it("tests the relay as typed in the settings", async () => {
    await expect(invoke("collab_test_relay", { url: "", public: false })).rejects.toThrow(/Enter a relay server/);
    const ok = await invoke<RelayStatus>("collab_test_relay", { url: "relay.example.com", public: false });
    expect(ok).toEqual({ url: "https://relay.example.com", connected: true });
    await expect(invoke("collab_test_relay", { url: "https://unreachable.example.com", public: false })).rejects.toThrow(
      /Cannot reach the relay server/,
    );
  });

  it("shows a session on demand", async () => {
    const seen: SessionView[] = [];
    await listen<SessionView>(STATUS_EVENT, (s) => seen.push(s));
    const view: SessionView = { role: "host", canHost: true, participants: [], files: [] };
    window.__forgeCollabMock!.show(view);
    expect(seen).toEqual([view]);
  });
});
