import { describe, expect, it } from "vitest";
import "./workspaceMock";
import { workspaceApi } from "@/ipc/workspace";

describe("browser-mode workspace backend", () => {
  it("lists directories before files", async () => {
    const root = await workspaceApi.listDir("");
    expect(root[0].kind).toBe("dir");
    expect(root.map((e) => e.name)).toContain("composer.json");
  });

  it("round-trips writes", async () => {
    await workspaceApi.writeFile("scratch.php", "<?php echo 1;");
    expect(await workspaceApi.readFile("scratch.php")).toBe("<?php echo 1;");
  });
});
