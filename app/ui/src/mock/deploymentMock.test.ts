import { describe, expect, it } from "vitest";
import "./settingsMock";
import "./workspaceMock";
import "./trustMock";
import "./deploymentMock";
import { listen } from "@/ipc";
import { blankServer, deploymentApi, TRANSFER_EVENT, type TransferEvent } from "@/ipc/deployment";

describe("browser-mode deployment", () => {
  it("lists the demo servers and validates servers in Rust", async () => {
    const config = await deploymentApi.config();
    expect(config.defaultServer).toBe("shop-production");
    expect(config.servers.map((s) => s.kind)).toEqual(["sftp", "ftps", "local"]);
    const problems = await deploymentApi.validate({ ...blankServer("ftp"), name: "x", webUrl: "shop.example.com" });
    expect(problems).toEqual([
      "Specify the host",
      "Specify the user name or log in anonymously",
      "The web server URL must start with http:// or https://",
    ]);
  });

  it("uploads with progress events and asks before overwriting newer files", async () => {
    const events: TransferEvent[] = [];
    const off = await listen<TransferEvent>(TRANSFER_EVENT, (e) => events.push(e));
    const first = await deploymentApi.transfer("t1", null, "upload", ["app/Models"]);
    expect(first.newerOnTarget).toEqual(["app/Models/User.php"]);
    expect(first.transferred).toBe(0);
    const second = await deploymentApi.transfer("t2", null, "upload", first.newerOnTarget, true);
    expect(second.transferred).toBe(1);
    expect(second.results[0].remote).toBe("/var/www/shop/app/Models/User.php");
    expect(events.filter((e) => e.kind === "started").map((e) => e.id)).toEqual(["t1", "t2"]);
    expect(events.at(-1)).toMatchObject({ kind: "finished", id: "t2" });
    off();
  });

  it("compares with the deployed version through the Rust folder diff", async () => {
    const sync = await deploymentApi.syncCompare("shop-production", "public");
    expect(sync.remote).toBe("/var/www/shop/public");
    const maintenance = sync.compare.entries.find((e) => e.path === "maintenance.html");
    expect(maintenance?.status).toBe("rightOnly");
    const remote = await deploymentApi.readRemote(null, "public/maintenance.html");
    expect(remote.text).toContain("Back soon");
    await expect(deploymentApi.readRemote(null, "storage/logs/laravel.log")).rejects.toThrow(/excluded/);
  });

  it("uploads automatically only when the option says so", async () => {
    expect(await deploymentApi.autoUpload("a1", ["app/Models/User.php"], true)).toBeNull();
  });
});
