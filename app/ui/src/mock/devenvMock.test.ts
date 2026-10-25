import { describe, expect, it } from "vitest";
import "./workspaceMock";
import "./trustMock";
import "./devenvMock";
import { devcontainerApi, wslApi, type BuildLogLine } from "@/ipc/devenv";
import { absolutePath } from "@/features/devenv/gutter";

describe("browser-mode dev containers and WSL", () => {
  it("resolves the demo devcontainer.json in Rust", async () => {
    const [view] = await devcontainerApi.configs();
    expect(view.rel).toBe(".devcontainer/devcontainer.json");
    expect(view.source).toContain("Dockerfile .devcontainer/Dockerfile");
    expect(view.config).toMatchObject({
      name: "Acme Shop",
      workspaceFolder: "/workspaces/acme-shop",
      remoteUser: "vscode",
      forwardPorts: [{ port: 8000 }, { port: 5173 }],
    });
    expect(view.config?.features.map((f) => f.id)).toEqual(["ghcr.io/devcontainers/features/node:1"]);
    expect(await devcontainerApi.gutter(".devcontainer/devcontainer.json", "// c\n{\n}")).toBe(1);
    expect(await devcontainerApi.gutter("composer.json", "{}")).toBeNull();
  });

  it("lists, builds, opens and manages containers", async () => {
    const before = await devcontainerApi.list();
    expect(before.available).toBe(true);
    expect(before.containers.map((c) => c.name)).toEqual(["Acme Shop", "Billing API"]);
    expect(before.containers[1]).toMatchObject({ state: "exited", composeProject: "billing-api_devcontainer" });

    const lines: BuildLogLine[] = [];
    const unlisten = await devcontainerApi.onLog((l) => lines.push(l));
    const [view] = await devcontainerApi.configs();
    const outcome = await devcontainerApi.up(view.config!.localFolder, view.path, true);
    unlisten();
    expect(outcome.created).toBe(true);
    expect(lines.some((l) => l.kind === "command" && l.text.startsWith("docker build"))).toBe(true);
    expect(lines.at(-1)).toEqual({ kind: "step", text: "Dev container is ready." });

    const info = await devcontainerApi.open(outcome.containerId);
    expect(info.root).toBe("Acme Shop [Dev Container]:/workspaces/acme-shop");
    expect(info.remote?.kind).toMatchObject({ type: "devContainer", source: "~/Projects/acme-shop" });

    const billing = before.containers[1].containerId;
    await devcontainerApi.action(billing, "start");
    expect((await devcontainerApi.list()).containers.find((c) => c.containerId === billing)?.state).toBe("running");
    await devcontainerApi.action(billing, "remove");
    expect((await devcontainerApi.list()).containers.some((c) => c.containerId === billing)).toBe(false);
  });

  it("refuses to build untrusted folders", async () => {
    await expect(devcontainerApi.up("~/Downloads/untrusted", "~/Downloads/untrusted/.devcontainer.json")).rejects.toThrow(
      /Safe Mode/,
    );
  });

  it("parses WSL distributions from UTF-16 and maps paths", async () => {
    const info = await wslApi.distributions();
    expect(info.distros.map((d) => d.name)).toEqual(["Ubuntu-24.04", "Debian"]);
    expect(info.distros[0].default).toBe(true);
    expect(await wslApi.mapPath("\\\\wsl$\\Ubuntu-24.04\\home\\demo\\shop")).toEqual({
      distro: "Ubuntu-24.04",
      path: "/home/demo/shop",
    });
    expect(await wslApi.mapPath("C:\\src")).toEqual({ path: "/mnt/c/src" });
    expect(await wslApi.windowsPath("Debian", "/mnt/d/x")).toBe("D:\\x");

    const c = await wslApi.connect("Ubuntu-24.04");
    const home = await wslApi.browse(c.connectionId, "~");
    expect(home.path).toBe("/home/demo");
    const projects = await wslApi.browse(c.connectionId, "/home/demo/projects");
    expect(projects.entries.find((e) => e.name === "acme-shop")?.project).toBe(true);
    const ws = await wslApi.openProject(c.connectionId, "Ubuntu-24.04", "/home/demo/projects/acme-shop");
    expect(ws.root).toBe("\\\\wsl.localhost\\Ubuntu-24.04\\home\\demo\\projects\\acme-shop");
    expect(ws.remote?.label).toBe("WSL: Ubuntu-24.04");
  });

  it("makes gutter paths absolute", () => {
    expect(absolutePath("/src/shop", ".devcontainer/devcontainer.json")).toBe("/src/shop/.devcontainer/devcontainer.json");
    expect(absolutePath("C:\\src\\shop", ".devcontainer/devcontainer.json")).toBe("C:\\src\\shop\\.devcontainer\\devcontainer.json");
    expect(absolutePath("/src/shop", "/abs/devcontainer.json")).toBe("/abs/devcontainer.json");
  });
});
