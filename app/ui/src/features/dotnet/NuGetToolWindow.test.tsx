import { describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import "@/mock/settingsMock";
import "@/mock/keymapMock";
import type { DotnetWorkspace } from "@/ipc/dotnet";

const mocks = vi.hoisted(() => ({
  versions: new Map<string, (list: string[]) => void>(),
}));

vi.mock("@/ipc/dotnet", async (orig) => {
  const real = await orig<typeof import("@/ipc/dotnet")>();
  const workspace = {
    solutions: [],
    configuration: "Debug",
    projects: [
      {
        name: "Api",
        path: "src/Api/Api.csproj",
        packages: [
          { id: "Alpha", version: "1.0.0" },
          { id: "Beta", version: "2.0.0" },
        ],
      },
    ],
  } as unknown as DotnetWorkspace;
  return {
    ...real,
    dotnetApi: {
      ...real.dotnetApi,
      workspace: () => Promise.resolve(workspace),
      nugetUpdates: () => Promise.resolve([]),
      nugetVersions: (id: string) => new Promise<string[]>((resolve) => mocks.versions.set(id, resolve)),
    },
  };
});

const { NuGetToolWindow } = await import("./NuGetToolWindow");

const tick = () => new Promise((r) => setTimeout(r, 0));

describe("NuGet tool window", () => {
  it("shows the versions of the selected package when an older reply arrives late", async () => {
    // Attached: Solid delegates clicks to the document.
    const host = document.body.appendChild(document.createElement("div"));
    const dispose = render(() => <NuGetToolWindow />, host);
    await tick();
    const row = (id: string) => host.querySelector<HTMLElement>(`[data-package="${id}"]`)!;
    const options = () => [...host.querySelectorAll<HTMLOptionElement>('[data-testid="nuget-version"] option')].map((o) => o.value);

    row("Alpha").click();
    await tick();
    row("Beta").click();
    await tick();
    expect([...mocks.versions.keys()]).toEqual(["Alpha", "Beta"]);
    mocks.versions.get("Beta")!(["2.0.0", "2.1.0"]);
    await tick();
    expect(options()).toEqual(["2.1.0", "2.0.0"]);
    // Alpha's versions were asked for first and arrive last: Beta is still selected.
    mocks.versions.get("Alpha")!(["1.0.0", "1.1.0"]);
    await tick();
    expect(options()).toEqual(["2.1.0", "2.0.0"]);
    dispose();
    host.remove();
  });
});
