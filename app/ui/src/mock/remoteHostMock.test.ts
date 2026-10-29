import { describe, expect, it } from "vitest";
import "./settingsMock";
import "./workspaceMock";
import "./trustMock";
import "./remoteMock";
import "./deploymentMock";
import "./remoteHostMock";
import "./terminalMock";
import { listen } from "@/ipc";
import { remoteHostApi } from "@/ipc/remoteHost";
import { remoteApi } from "@/ipc/remote";
import { deploymentApi } from "@/ipc/deployment";
import { terminalApi, type TerminalEvent } from "@/ipc/terminal";
import { sshChoices, sshTabTitle } from "@/features/terminal/ssh";

describe("browser-mode Remote Host", () => {
  it("lists the server tree with mappings from Rust", async () => {
    const top = await remoteHostApi.list("shop-production", "");
    expect(top.root).toBe("/var/www/shop");
    expect(top.rootNode).toMatchObject({ path: "", name: "/var/www/shop", mappingRoot: true, local: "" });
    const dirs = top.nodes.filter((n) => n.dir).map((n) => n.name);
    expect(dirs).toContain("public");
    // Folders come first.
    const firstFile = top.nodes.findIndex((n) => !n.dir);
    expect(firstFile).toBeGreaterThan(0);
    expect(top.nodes.slice(firstFile).every((n) => !n.dir)).toBe(true);
    const pub = top.nodes.find((n) => n.name === "public")!;
    expect(pub).toMatchObject({ mappingRoot: true, local: "public" });
    const inner = await remoteHostApi.list("shop-production", "public");
    expect(inner.nodes.find((n) => n.name === "maintenance.html")).toMatchObject({ local: "public/maintenance.html", dir: false });
    const uploads = await remoteHostApi.create("shop-production", "public", "uploads", true);
    expect(uploads.excluded).toBe(true);
  });

  it("creates, renames, edits with a conflict check and deletes", async () => {
    await expect(remoteHostApi.create("shop-production", "public", "a/b", false)).rejects.toThrow("must not contain");
    const node = await remoteHostApi.create("shop-production", "public", "robots.txt", false);
    expect(node.path).toBe("public/robots.txt");
    await expect(remoteHostApi.create("shop-production", "public", "robots.txt", false)).rejects.toThrow("already exists");
    expect(await remoteHostApi.rename("shop-production", "public/robots.txt", "humans.txt")).toBe("public/humans.txt");

    const opened = await remoteHostApi.edit("shop-production", "public/humans.txt");
    expect(opened).toMatchObject({ text: "", remote: "/var/www/shop/public/humans.txt" });
    expect(await remoteHostApi.saveEdit("shop-production", "public/humans.txt", "Team: Forge")).toEqual({ status: "uploaded" });
    // Someone else changes the file: the next save asks.
    await deploymentApi.transfer("rh-1", "shop-production", "upload", ["public/index.php"], true);
    const other = await remoteHostApi.edit("shop-production", "public/index.php");
    await remoteHostApi.rename("shop-production", "public/index.php", "index2.php");
    expect(await remoteHostApi.saveEdit("shop-production", "public/index.php", other.text)).toEqual({ status: "conflict", deleted: true });
    expect(await remoteHostApi.saveEdit("shop-production", "public/index.php", other.text, true)).toEqual({ status: "uploaded" });

    expect(await remoteHostApi.delete("shop-production", ["public/humans.txt", "public/index2.php"])).toBe(2);
    await expect(remoteHostApi.delete("shop-production", [""])).rejects.toThrow("never deleted");
  });
});

describe("SSH sessions", () => {
  it("offers SSH configurations and SFTP deployment servers", async () => {
    const [configs, deploy] = await Promise.all([remoteApi.configs(), deploymentApi.config()]);
    const choices = sshChoices(configs, deploy.servers);
    expect(choices.map((c) => `${c.kind}:${c.id}`)).toEqual(["config:staging", "config:shop-key", "server:shop-production"]);
    expect(choices[2]).toMatchObject({ title: "shop-production", detail: "deploy@shop.example.com", host: "shop.example.com" });
    expect(sshTabTitle(" shop.example.com ")).toBe("shop.example.com");
  });

  it("opens a shell over SSH with the host's prompt", async () => {
    const output: string[] = [];
    const off = await listen<TerminalEvent>("terminal://ssh-1", (e) => e.type === "output" && output.push(e.data));
    const opened = await terminalApi.open({ id: "ssh-1", ssh: { serverId: "shop-production" }, cols: 80, rows: 24 });
    expect(opened.shell).toBe("deploy@shop.example.com");
    await new Promise((r) => setTimeout(r, 60));
    expect(output.join("")).toContain("deploy@shop");
    await expect(terminalApi.open({ id: "ssh-2", ssh: { configId: "missing" }, cols: 80, rows: 24 })).rejects.toThrow("does not exist");
    off();
  });
});
