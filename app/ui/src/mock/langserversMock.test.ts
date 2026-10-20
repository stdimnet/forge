import { describe, expect, it } from "vitest";
import { invoke, listen } from "@/ipc";
import type { InstallLine, LicenceStatus, ServerRow, WorkspaceSymbol } from "@/ipc/langservers";
import "./settingsMock";
import { demoWorkspaceSymbols, mockRows } from "./langserversMock";

describe("language servers mock", () => {
  it("lists servers with mixed states, including the PHP pack's", async () => {
    const rows = await mockRows();
    const statuses = new Set(rows.map((r) => r.status));
    expect(statuses).toEqual(new Set(["running", "installed", "notInstalled", "disabled"]));
    const ip = rows.find((r) => r.id === "intelephense")!;
    expect(ip.source).toBe("php");
    expect(rows.filter((r) => r.source === "catalog").length).toBeGreaterThanOrEqual(30);
  });

  it("follows the per-server settings", async () => {
    await invoke("settings_set", { key: "langservers.servers.gopls.enabled", value: false, scope: "user" });
    await invoke("settings_set", { key: "langservers.servers.gopls.settings", value: "{", scope: "user" });
    const gopls = (await invoke<ServerRow[]>("langservers_list")).find((r) => r.id === "gopls")!;
    expect(gopls.status).toBe("disabled");
    expect(gopls.extraSettingsError).toMatch(/invalid JSON/);
    await invoke("settings_set", { key: "langservers.servers.gopls.enabled", value: null, scope: "user" });
    await invoke("settings_set", { key: "langservers.servers.gopls.settings", value: null, scope: "user" });
  });

  it("streams install output and marks the server installed", async () => {
    const lines: InstallLine[] = [];
    const off = await listen<InstallLine>("langservers://install-output", (l) => lines.push(l));
    const outcome = await invoke("langservers_install", { id: "docker-langserver", index: 0 });
    off();
    expect(outcome).toBe("succeeded");
    expect(lines[0].line).toBe(
      "$ npm install --prefix /home/marlon/.local/share/forge/langservers --no-fund --no-audit dockerfile-language-server-nodejs",
    );
    expect(lines.some((l) => l.stream === "stderr")).toBe(true);
    const row = (await mockRows()).find((r) => r.id === "docker-langserver")!;
    expect(row.status).toBe("installed");
    expect(row.path).toBe("/home/marlon/.local/share/forge/langservers/node_modules/.bin/docker-langserver");
    await expect(invoke("langservers_install", { id: "zls", index: 0 })).rejects.toThrow(/brew was not found/);
  }, 10_000);

  it("finds workspace symbols in several languages", async () => {
    const all = demoWorkspaceSymbols();
    const servers = new Set(all.map((s) => s.server));
    expect(servers.has("typescript-language-server")).toBe(true);
    expect(servers.has("vscode-css-language-server")).toBe(true);
    expect(servers.has("yaml-language-server")).toBe(true);
    const hits = await invoke<WorkspaceSymbol[]>("lsp_workspace_symbols", { query: "order" });
    expect(hits.map((h) => h.name)).toEqual(expect.arrayContaining(["Order", "loadOrders"]));
    expect(hits.find((h) => h.name === "Order")).toMatchObject({ kind: "interface", path: "resources/js/app.ts" });
  });

  it("switches Intelephense between Free and Premium", async () => {
    expect((await invoke<LicenceStatus>("intelephense_licence_status")).premium).toBe(false);
    await expect(invoke("intelephense_set_licence", { key: "bad key" })).rejects.toThrow(/licence key/);
    const premium = await invoke<LicenceStatus>("intelephense_set_licence", { key: "ABCDEFGH1234" });
    expect(premium).toMatchObject({ premium: true, maskedKey: "…1234" });
    expect((await invoke<LicenceStatus>("intelephense_set_licence", { key: null })).premium).toBe(false);
  });
});
