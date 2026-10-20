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
});
