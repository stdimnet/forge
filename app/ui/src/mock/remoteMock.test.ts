import { describe, expect, it } from "vitest";
import "./workspaceMock";
import { remoteApi, type SshConfiguration } from "@/ipc/remote";
import { latencyLevel, statusText } from "@/features/remote/RemoteStatusItem";
import { keyTypeLabel } from "@/features/remote/HostKeyDialog";

const shop: SshConfiguration = { id: "", host: "shop", port: 22, user: "deploy", auth: "password" };

describe("browser-mode remote development", () => {
  it("lists ~/.ssh/config hosts and validates configurations in Rust", async () => {
    const hosts = await remoteApi.sshHosts();
    expect(hosts.map((h) => h.alias)).toEqual(["shop", "staging"]);
    expect(hosts[1]).toMatchObject({ hostName: "staging.shop.internal", port: 2222, user: "root" });
    expect(await remoteApi.validate({ ...shop, host: "" })).toContain("Host is required");
  });

  it("asks for the host key once, then connects, browses and opens the project", async () => {
    const first = await remoteApi.connect(shop, "secret");
    expect(first.status).toBe("failed");
    if (first.status !== "failed" || first.error.kind !== "hostKey") throw new Error("expected a host key question");
    expect(first.error.hostKey).toMatchObject({ status: "unknown", host: "shop.example.com", keyType: "ssh-ed25519" });
    expect(first.error.hostKey.fingerprint).toMatch(/^SHA256:/);

    const ok = await remoteApi.connect(shop, "secret", first.error.hostKey.fingerprint);
    expect(ok.status).toBe("connected");
    if (ok.status !== "connected") return;
    // Known now: the next connection needs no confirmation.
    expect((await remoteApi.test(shop, "secret")).ok).toBe(true);

    const www = await remoteApi.browse(ok.connectionId, "/var/www/../www");
    expect(www.path).toBe("/var/www");
    expect(www.entries.find((e) => e.name === "shop")).toMatchObject({ dir: true, project: true });
    const project = await remoteApi.browse(ok.connectionId, "/var/www/shop");
    expect(project.entries.map((e) => e.name)).toContain("composer.json");

    const info = await remoteApi.openProject(ok.connectionId, "/var/www/shop");
    // Shown with the host as typed (an ~/.ssh/config alias here), like the backend does.
    expect(info.root).toBe("deploy@shop:/var/www/shop");
    expect(info.remote).toMatchObject({ host: "shop", user: "deploy", path: "/var/www/shop" });
    expect(await remoteApi.status()).toMatchObject({ state: "connected" });
  });

  it("reports wrong credentials", async () => {
    const r = await remoteApi.test({ ...shop, host: "staging.shop.internal", port: 2222, user: "root", id: "" });
    expect(r.ok).toBe(false);
    expect(r.error?.kind).toBe("auth");
  });

  it("formats the status widget", () => {
    expect(latencyLevel(38)).toBe("good");
    expect(latencyLevel(400)).toBe("poor");
    expect(
      statusText({ connectionId: "c", label: "l", host: "h", state: "reconnecting", attempt: 2, error: "x", nextRetrySecs: 2 }),
    ).toBe("Reconnecting… (attempt 2)");
    expect(keyTypeLabel("ssh-ed25519")).toBe("ED25519");
    expect(keyTypeLabel("ecdsa-sha2-nistp256")).toBe("ECDSA");
  });
});
