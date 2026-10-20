import { describe, expect, it } from "vitest";
import { invoke, listen } from "@/ipc";
import { COMPANION_OPEN_EVENT, type CompanionStatus, type OpenEvent, type ProjectProfile } from "@/ipc/companion";
import "./workspaceMock";
import "./companionMock";

describe("companion mock", () => {
  it("pairs and unpairs browsers", async () => {
    const code = await invoke<{ code: string }>("companion_start_pairing");
    expect(code.code).toMatch(/^\d{6}$/);
    let status = await invoke<CompanionStatus>("companion_status");
    expect(status.pairing?.code).toBe(code.code);
    expect(status.server.running).toBe(true);
    await invoke("companion_revoke_client", { id: status.clients[0].id });
    status = await invoke<CompanionStatus>("companion_status");
    expect(status.clients).toHaveLength(0);
  });

  it("profiles the demo project and applies dotenv changes in memory", async () => {
    const before = await invoke<ProjectProfile>("companion_project_profile", { envKeys: ["IGNITION_EDITOR"], files: [] });
    expect(before.composer).toContain("laravel/framework");
    expect(before.npm).toContain("vite");
    expect(before.env[".env"].IGNITION_EDITOR).toBeUndefined();

    await invoke("companion_set_env_var", { file: ".env", key: "IGNITION_EDITOR", value: "forge" });
    const after = await invoke<ProjectProfile>("companion_project_profile", { envKeys: ["IGNITION_EDITOR"], files: [] });
    expect(after.env[".env"].IGNITION_EDITOR).toBe("forge");
    const env = await invoke<string>("fs_read_file", { path: ".env" });
    expect(env).toContain("APP_NAME=");
    expect(env.trimEnd().endsWith("IGNITION_EDITOR=forge")).toBe(true);
  });

  it("opens forge:// links", async () => {
    const events: OpenEvent[] = [];
    const un = await listen<OpenEvent>(COMPANION_OPEN_EVENT, (e) => events.push(e));
    await invoke("companion_open_link", { url: "forge://open?file=app/Models/User.php&line=42" });
    un();
    expect(events[0]).toMatchObject({ source: "link", line: 42, target: { kind: "open", path: "app/Models/User.php" } });
  });
});
