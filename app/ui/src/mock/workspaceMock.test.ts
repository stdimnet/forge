import { describe, expect, it } from "vitest";
import "./workspaceMock";
import { workspaceApi } from "@/ipc/workspace";

describe("browser-mode workspace backend", () => {
  it("lists directories before files", async () => {
    const root = await workspaceApi.listDir("");
    expect(root[0].kind).toBe("dir");
    expect(root.map((e) => e.name)).toContain("composer.json");
  });

  it("flags ignored untracked entries and keeps tracked ones normal", async () => {
    const root = await workspaceApi.listDir("");
    const flag = (name: string) => root.find((e) => e.name === name)?.ignored;
    expect(flag("vendor")).toBe(true);
    expect(flag(".env")).toBe(true);
    // Matched by `*.log` but force-added: tracked, so it keeps its normal VCS status.
    expect(flag("forced.log")).toBe(false);
    expect(flag("composer.json")).toBe(false);
    const vendor = await workspaceApi.listDir("vendor");
    expect(vendor.every((e) => e.ignored)).toBe(true);
  });

  it("round-trips writes", async () => {
    await workspaceApi.writeFile("scratch.php", "<?php echo 1;");
    expect(await workspaceApi.readFile("scratch.php")).toBe("<?php echo 1;");
  });

  it("creates, renames and deletes files and folders", async () => {
    await workspaceApi.createFile("new/Thing.php");
    await workspaceApi.createDir("empty/inner");
    expect((await workspaceApi.listDir("")).map((e) => e.name)).toEqual(expect.arrayContaining(["new", "empty"]));
    expect((await workspaceApi.listDir("empty")).map((e) => e.name)).toEqual(["inner"]);
    await expect(workspaceApi.createFile("new/Thing.php")).rejects.toThrow("already exists");

    await workspaceApi.rename("new", "renamed");
    expect(await workspaceApi.readFile("renamed/Thing.php")).toBe("");
    await expect(workspaceApi.readFile("new/Thing.php")).rejects.toThrow();
    await expect(workspaceApi.rename("renamed", "renamed/inside")).rejects.toThrow("into itself");

    await workspaceApi.delete("renamed");
    await workspaceApi.delete("empty");
    const names = (await workspaceApi.listDir("")).map((e) => e.name);
    expect(names).not.toContain("renamed");
    expect(names).not.toContain("empty");
  });
});
