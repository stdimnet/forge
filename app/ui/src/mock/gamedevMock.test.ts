import { describe, expect, it } from "vitest";
import type { BridgeEvent, UnityStatus } from "@/ipc/gamedev";
import type { ProjectInfo } from "@/ipc/project";

// The Unity demo is chosen by the page URL, before the mocks load.
history.replaceState(null, "", "/?project=unity");
const { invoke, listen } = await import("@/ipc");
await import("./settingsMock");
await import("./workspaceMock");
await import("./projectMock");
await import("./gamedevMock");

describe("Unity mock", () => {
  it("detects the demo and its editor", async () => {
    const info = await invoke<ProjectInfo>("project_detect");
    expect(info.primary).toMatchObject({ id: "unity", version: "2022.3.10f1" });
    const s = await invoke<UnityStatus>("unity_status");
    expect(s.editor?.version).toBe("2022.3.10f1");
    expect(s.exact).toBe(true);
    expect(s.bridgeAvailable).toBe(true);
    expect(s.project?.bridge.kind).toBe("embedded");
  });

  it("connects to the simulated editor and streams its state", async () => {
    const events: BridgeEvent[] = [];
    await listen<BridgeEvent>("unity://bridge", (e) => events.push(e));
    const w = await invoke<{ unityVersion: string }>("unity_bridge_connect");
    expect(w.unityVersion).toBe("2022.3.10f1");
    expect(await invoke("unity_bridge_command", { command: "pause" })).toEqual({ ok: true });
    expect(events).toContainEqual({ type: "state", playMode: "paused", compiling: false });
    expect((await invoke<{ ok: boolean }>("unity_bridge_command", { command: "syncSolution" })).ok).toBe(true);
    // Unity has the project open: no batch mode.
    await expect(invoke("unity_command", { action: "syncSolution" })).rejects.toThrow(/connect to it/);
    // Attaching needs a Mono debug adapter.
    await expect(invoke("unity_attach_request", { port: 56242 })).rejects.toThrow(/Mono/);
  });
});
