import { describe, expect, it, vi } from "vitest";
import { invoke, listen } from "@/ipc";
import { PRESETS, STATUS_EVENT, type RelayStatus, type RelayTokenStatus, type SessionView } from "@/ipc/collab";
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

describe("Code With Me mock: host approval", () => {
  it("lets the guest in only after the host accepted", async () => {
    const requests: { id: number; name: string }[] = [];
    await listen<{ id: number; name: string }>("collab://join-request", (r) => requests.push(r));
    await invoke("collab_start", { level: "readOnly" });
    await vi.waitFor(() => expect(requests.length).toBeGreaterThan(0), { timeout: 3000 });
    expect(requests[0].name).toBe("Alex");
    const status = await invoke<{ participants: { name: string }[] }>("collab_status");
    expect(status.participants.map((p) => p.name)).not.toContain("Alex");
    await invoke("collab_answer_join", { id: requests[0].id, accept: true });
    await vi.waitFor(async () => expect((await invoke<{ participants: { name: string }[] }>("collab_status")).participants.map((p) => p.name)).toContain("Alex"));
    await invoke("collab_end");
  });
});

describe("Code With Me mock: during a session", () => {
  type Status = SessionView & { participants: { id: number; name: string; permissions: { files: string } }[] };
  const status = () => invoke<Status>("collab_status");

  it("changes permissions, removes a guest and regenerates the link", async () => {
    const requests: { id: number }[] = [];
    await listen<{ id: number }>("collab://join-request", (r) => requests.push(r));
    const custom = { files: "edit", terminal: "none", run: "none", vcs: "full" } as const;
    const started = await invoke<SessionView>("collab_start", { level: "custom", custom });
    expect(started.permissions).toEqual(custom);
    await vi.waitFor(() => expect(requests.length).toBeGreaterThan(0), { timeout: 3000 });
    await invoke("collab_answer_join", { id: requests[requests.length - 1].id, accept: true });
    await vi.waitFor(async () => expect((await status()).participants.length).toBe(2));
    const alex = (await status()).participants[1];
    expect(alex.permissions).toEqual(custom);

    await invoke("collab_set_guest_permissions", { participant: alex.id, permissions: PRESETS.fullAccess });
    expect((await status()).participants[1].permissions).toEqual(PRESETS.fullAccess);
    const v = await invoke<SessionView>("collab_set_permissions", { permissions: PRESETS.readOnly });
    expect(v.permissions).toEqual(PRESETS.readOnly);
    expect((await status()).participants[1].permissions).toEqual(PRESETS.readOnly);

    const old = (await status()).link;
    const fresh = await invoke<string>("collab_regenerate_link");
    expect(fresh).not.toBe(old);
    expect((await status()).link).toBe(fresh);

    await invoke("collab_remove_guest", { participant: alex.id });
    expect((await status()).participants.map((p) => p.name)).toEqual(["Robert"]);
    await expect(invoke("collab_remove_guest", { participant: alex.id })).rejects.toThrow(/no longer in the session/);
    await invoke("collab_end");
    const log = await invoke<{ lines: { message: string }[] }>("collab_session_log");
    expect(log.lines.map((l) => l.message)).toEqual([
      "Robert started the session (guests: Custom)",
      "Alex joined (Custom)",
      "Permission of Alex: Full access",
      "Permission of all guests: Read-only",
      "Regenerated the session link",
      "Removed Alex",
      "Session ended: The host ended the session.",
    ]);
  });
});
