import { describe, expect, it } from "vitest";
import type { BuildReport, DotnetWorkspace, PackageUpdate, SearchResult } from "@/ipc/dotnet";
import type { CommandLine } from "@/ipc/run";
import type { TestFile } from "@/ipc/testFrameworks";

// The .NET demo is chosen by the page URL, before the mocks load.
history.replaceState(null, "", "/?project=dotnet");
const { invoke } = await import("@/ipc");
const { dotnetDemoFiles } = await import("./demoDotnetProject");
await import("./settingsMock");
await import("./workspaceMock");
await import("./testFrameworksMock");
await import("./dotnetMock");
const { dotnetCommandSteps, dotnetTestSteps } = await import("./dotnetTestsMock");
const { forgeWasm } = await import("@/wasm");

describe("DotNetForge mock (Rust crates through WebAssembly)", () => {
  it("reads the solution and its projects", async () => {
    const ws = await invoke<DotnetWorkspace>("dotnet_workspace", { refresh: true });
    expect(ws.solution).toBe("Bookstore.sln");
    expect(ws.projects.map((p) => p.name)).toEqual(["Bookstore.Api", "Bookstore.Core", "Bookstore.Tests", "Reports"]);
    const api = ws.projects[0]!;
    expect(api.web).toBe(true);
    expect(api.launchProfiles?.map((p) => p.name)).toEqual(["http", "https", "IIS Express"]);
    expect(api.packages.find((p) => p.id === "Swashbuckle.AspNetCore")).toMatchObject({ version: "6.5.0", source: "central" });
    expect(ws.projects[2]!.testFramework).toBe("xunit");
  });

  it("builds command lines and reads build output", async () => {
    const build = await invoke<CommandLine>("dotnet_command", { action: "build" });
    expect(build.args).toEqual(["build", "Bookstore.sln", "-c", "Debug", "-nologo", "-v", "minimal"]);
    const output = dotnetCommandSteps(["build"])
      .map((s) => s[1])
      .filter((t): t is string => typeof t === "string")
      .join("");
    const report = await invoke<BuildReport>("dotnet_build_problems", { output });
    expect(report.succeeded).toBe(true);
    expect(report.outputs.length).toBe(5);
    const add = await invoke<CommandLine>("nuget_command", { action: "add", project: "src/Bookstore.Api/Bookstore.Api.csproj", package: "Serilog", version: "4.1.0" });
    expect(add.args).toEqual(["add", "src/Bookstore.Api/Bookstore.Api.csproj", "package", "Serilog", "--version", "4.1.0"]);
  });

  it("discovers xUnit tests and runs them with a TRX report", async () => {
    const path = "tests/Bookstore.Tests/CartTests.cs";
    const found = await invoke<TestFile>("test_discover", { path, text: dotnetDemoFiles[path] });
    expect(found.framework).toBe("dotnet-test");
    expect(found.targets.map((t) => t.key)).toContain(`${path}::Total_AddsLines`);
    const steps = dotnetTestSteps({ name: "CartTests", type: "dotnet-test", options: { scope: "class", class: "Bookstore.Tests.CartTests" } });
    const report = steps.find((s) => typeof s[1] === "object" && "report" in s[1])![1] as { report: string };
    const wasm = await forgeWasm();
    expect(report.report).toContain("<TestRun");
    expect(typeof wasm.dotnet_discover).toBe("function");
    expect(steps.at(-1)?.[1]).toEqual({ code: 1 });
  });

  it("searches NuGet and finds upgrades", async () => {
    const r = await invoke<SearchResult>("nuget_search", { query: "serilog" });
    expect(r.packages.map((p) => p.id)).toContain("Serilog.AspNetCore");
    expect(r.packages.find((p) => p.id === "Serilog.Sinks.RollingFile")?.deprecated).toBe(true);
    const updates = await invoke<PackageUpdate[]>("nuget_updates", {});
    expect(updates.find((u) => u.id === "Swashbuckle.AspNetCore")).toEqual({ id: "Swashbuckle.AspNetCore", installed: "6.5.0", latest: "6.9.0" });
  });
});
