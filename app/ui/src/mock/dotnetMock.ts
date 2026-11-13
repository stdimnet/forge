/**
 * Browser-mode DotNetForge for the `Bookstore` demo (`?project=dotnet`): the .NET host and
 * SDKs of this "machine", the solution and its projects, build / run / NuGet / template /
 * cleanup command lines, build problems, test discovery and NuGet search answers —
 * computed by the Rust crates through WebAssembly — and canned output. Installing,
 * removing packages and adding projects change the in-memory demo files only.
 */
import { emitMock, registerMock } from "@/ipc";
import type { LaunchRequest } from "@/features/debug/dapApi";
import type { DotnetStatus, NewProjectRequest, PackageUpdate, SearchResult } from "@/ipc/dotnet";
import type { CommandLine, RunConfiguration } from "@/ipc/run";
import { callWasm, forgeWasm } from "@/wasm";
import { dotnetBrokenBuild, dotnetDemoInfo, dotnetDemoName, isDotnetDemo } from "./demoDotnetProject";
import { onMockDidOpen, setMockServerStatus } from "./lspMock";
import { DOTNET, DOTNET_ROOT, dotnetContext, loadDemoWorkspace } from "./dotnetTestsMock";
import { mockSettingValue, setSetting } from "./settingsMock";
import { mockFiles } from "./workspaceMock";

const ROOT = `~/Projects/${dotnetDemoName}`;
const call = async <T>(fn: string, input: unknown): Promise<T> => {
  const wasm = await forgeWasm();
  return callWasm<T>((wasm as unknown as Record<string, (json: string) => string>)[fn], input);
};
const rel = (cwd: string) => (cwd ? `${DOTNET_ROOT}/${cwd}` : DOTNET_ROOT);
const toCommandLine = (c: CommandLine & { cwd?: string }): CommandLine => ({ ...c, cwd: c.cwd?.startsWith("/") ? c.cwd : rel(c.cwd ?? "") });

/** `?roslyn=missing`: the demo starts without the Roslyn server (the download offer). */
let roslynInstalled = typeof location === "undefined" || new URLSearchParams(location.search).get("roslyn") !== "missing";
let netcoredbgInstalled = true;

function status(): DotnetStatus {
  const sdk = (version: string) => ({ version, path: `/usr/share/dotnet/sdk/${version}` });
  const runtime = (name: string, version: string) => ({ name, version, path: `/usr/share/dotnet/shared/${name}/${version}` });
  return {
    host: { path: DOTNET, root: "/usr/share/dotnet", source: "PATH" },
    hosts: [{ path: DOTNET, root: "/usr/share/dotnet", source: "PATH" }],
    sdks: [sdk("8.0.404"), sdk("8.0.405"), sdk("9.0.101")],
    runtimes: [
      runtime("Microsoft.AspNetCore.App", "8.0.11"),
      runtime("Microsoft.AspNetCore.App", "9.0.0"),
      runtime("Microsoft.NETCore.App", "8.0.11"),
      runtime("Microsoft.NETCore.App", "9.0.0"),
    ],
    sdk: "8.0.405",
    requestedSdk: "8.0.400",
    problems: [],
    install: [
      { label: "Ubuntu / Debian (apt)", command: "sudo apt-get install -y dotnet-sdk-8.0" },
      { label: "Fedora / RHEL (dnf)", command: "sudo dnf install dotnet-sdk-8.0" },
      { label: "Arch Linux (pacman)", command: "sudo pacman -S dotnet-sdk" },
      { label: "Download from dotnet.microsoft.com", url: "https://dotnet.microsoft.com/download" },
    ],
    roslyn: roslynInstalled
      ? { path: "/home/anna/.local/share/forge/langservers/roslyn/5.0.0-2.26526.1/Microsoft.CodeAnalysis.LanguageServer.dll", source: "managed", version: "5.0.0-2.26526.1" }
      : null,
    roslynProblem: roslynInstalled ? null : "The Roslyn language server is not installed",
    roslynVersion: "5.0.0-2.26526.1",
    roslynSize: 59_909_005,
    netcoredbg: netcoredbgInstalled ? { path: "/home/anna/.local/share/forge/debuggers/netcoredbg/3.1.2-1054/netcoredbg", source: "managed", version: "3.1.2-1054" } : null,
    netcoredbgProblem: netcoredbgInstalled ? null : "netcoredbg is not installed",
    netcoredbgVersion: "3.1.2-1054",
    remote: false,
  };
}

registerMock("dotnet_status", () => status());

const install = async (id: "roslyn" | "netcoredbg", label: string, version: string, total: number) => {
  for (let done = 0; done <= total; done += total / 4) {
    emitMock("dotnet://install", { id, event: { kind: "progress", label, done, total } });
    await new Promise((r) => setTimeout(r, 150));
  }
  emitMock("dotnet://install", { id, event: { kind: "step", message: `Verified SHA-256 of ${id} ${version}.` } });
  return version;
};

registerMock("dotnet_roslyn_install", async () => {
  const v = await install("roslyn", "Downloading the Roslyn language server 5.0.0-2.26526.1", "5.0.0-2.26526.1", 59_909_005);
  roslynInstalled = true;
  return v;
});
registerMock("dotnet_netcoredbg_install", async () => {
  const v = await install("netcoredbg", "Downloading netcoredbg 3.1.2-1054", "3.1.2-1054", 6_500_000);
  netcoredbgInstalled = true;
  return v;
});

registerMock("dotnet_workspace", async () => {
  const ws = await loadDemoWorkspace(mockFiles);
  emitMock("dotnet://workspace", ws);
  return ws;
});

registerMock("dotnet_command", async ({ action, target }) =>
  toCommandLine(await call<CommandLine>("dotnet_action_command", { action, target, context: await dotnetContext(mockFiles) })),
);

registerMock("dotnet_build_problems", ({ output }) => call("dotnet_build_report", { output, root: DOTNET_ROOT }));

const NEW_LIST = `These templates matched your input:

Template Name                                 Short Name                  Language    Tags
--------------------------------------------  --------------------------  ----------  --------------------------------
ASP.NET Core Empty                            web                         [C#],F#     Web/Empty
ASP.NET Core gRPC Service                     grpc                        [C#]        Web/gRPC/API/Service
ASP.NET Core Web API                          webapi                      [C#],F#     Web/Web API/API/Service
ASP.NET Core Web App (Model-View-Controller)  mvc                         [C#],F#     Web/MVC
ASP.NET Core Web App (Razor Pages)            webapp,razor                [C#]        Web/MVC/Razor Pages
Blazor Web App                                blazor                      [C#]        Web/Blazor/WebAssembly
Class Library                                 classlib                    [C#],F#,VB  Common/Library
Console App                                   console                     [C#],F#,VB  Common/Console
dotnet gitignore file                         gitignore,.gitignore                    Config
EditorConfig file                             editorconfig,.editorconfig              Config
MSTest Test Project                           mstest                      [C#],F#,VB  Test/MSTest/Desktop/Web
NUnit 3 Test Project                          nunit                       [C#],F#,VB  Test/NUnit/Desktop/Web
Solution File                                 sln,solution                            Solution
Worker Service                                worker                      [C#],F#     Common/Worker/Web
xUnit Test Project                            xunit                       [C#],F#,VB  Test/xUnit/Desktop/Web
`;

registerMock("dotnet_templates", async () => ({ templates: await call("dotnet_templates", { text: NEW_LIST }), fromSdk: true, error: null }));

registerMock("dotnet_new_project", async ({ request }) => {
  const cmds = await call<(CommandLine & { cwd?: string })[]>("dotnet_new_project", { request, context: await dotnetContext(mockFiles) });
  // The demo files get the new project right away (the commands only print output).
  const r = request as NewProjectRequest;
  const dir = r.directory || r.name;
  const lang = r.language === "F#" ? "fs" : r.language === "VB" ? "vb" : "cs";
  const fw = r.framework || "net8.0";
  mockFiles[`${dir}/${r.name}.${lang}proj`] = `<Project Sdk="Microsoft.NET.Sdk">\n\n  <PropertyGroup>\n    <TargetFramework>${fw}</TargetFramework>\n  </PropertyGroup>\n\n</Project>\n`;
  mockFiles[`${dir}/Class1.${lang}`] = lang === "cs" ? `namespace ${r.name};\n\npublic class Class1\n{\n}\n` : `namespace ${r.name}\n`;
  if (r.solution?.endsWith(".sln") && mockFiles[r.solution]) {
    const line = `Project("{9A19103F-16F7-4668-BE54-9A1E7A4F7556}") = "${r.name}", "${dir.replace(/\//g, "\\")}\\${r.name}.${lang}proj", "{6B2E0C11-0000-4000-8000-00000000000${Object.keys(mockFiles).length % 10}}"\nEndProject\n`;
    mockFiles[r.solution] = mockFiles[r.solution]!.replace(/^Global$/m, `${line}Global`);
  }
  return cmds.map(toCommandLine);
});

// ---- NuGet -------------------------------------------------------------------------------------------

const pkg = (id: string, version: string, description: string, authors: string, downloads: number, versions: string[], extra: Record<string, unknown> = {}) => ({
  id,
  version,
  description,
  authors,
  totalDownloads: downloads,
  verified: true,
  projectUrl: `https://www.nuget.org/packages/${id}`,
  versions: versions.map((v) => ({ version: v })),
  ...extra,
});

const FEED = [
  pkg("Serilog", "4.1.0", "Simple .NET logging with fully-structured events", "Serilog Contributors", 1_543_210_987, ["3.1.1", "4.0.0", "4.0.1", "4.1.0"]),
  pkg("Serilog.AspNetCore", "8.0.3", "Serilog support for ASP.NET Core logging", "Microsoft, Serilog Contributors", 412_345_678, ["8.0.0", "8.0.1", "8.0.2", "8.0.3"]),
  pkg("Serilog.Sinks.Console", "6.0.0", "A Serilog sink that writes log events to the console/terminal.", "Serilog Contributors", 498_765_432, ["5.0.1", "6.0.0"]),
  pkg("Serilog.Sinks.File", "6.0.0", "Write Serilog events to text files in plain or JSON format.", "Serilog Contributors", 401_234_567, ["5.0.0", "6.0.0"]),
  pkg("Serilog.Sinks.RollingFile", "3.3.0", "The rolling file sink for Serilog - Simple .NET logging with fully-structured events", "Serilog Contributors", 98_765_432, ["3.3.0"], { deprecation: { message: "Use Serilog.Sinks.File" }, vulnerabilities: [{ severity: "1" }] }),
  pkg("Swashbuckle.AspNetCore", "6.9.0", "Swagger tools for documenting APIs built on ASP.NET Core", "Swashbuckle.AspNetCore", 512_345_678, ["6.4.0", "6.5.0", "6.6.2", "6.9.0"]),
  pkg("FluentAssertions", "6.12.2", "A very extensive set of extension methods that allow you to more naturally specify the expected outcome of a TDD or BDD-style unit test.", "Dennis Doomen, Jonas Nyrup", 456_789_012, ["6.11.0", "6.12.0", "6.12.1", "6.12.2"]),
  pkg("xunit", "2.9.2", "xUnit.net is a developer testing framework, built to support Test Driven Development.", "jnewkirk, bradwilson", 702_345_678, ["2.6.6", "2.7.0", "2.8.1", "2.9.2"]),
  pkg("Microsoft.NET.Test.Sdk", "17.11.1", "The MSbuild targets and properties for building .NET test projects.", "Microsoft", 1_012_345_678, ["17.8.0", "17.9.0", "17.10.0", "17.11.1"]),
  pkg("xunit.runner.visualstudio", "2.8.2", "Visual Studio 2022+ Test Explorer runner for the xUnit.net framework.", "jnewkirk, bradwilson", 654_321_987, ["2.5.7", "2.5.8", "2.8.2"]),
  pkg("FSharp.Data", "6.4.0", "Library of F# type providers and data access tools", "Tomas Petricek, Gustavo Guerra, Colin Bull", 12_345_678, ["6.3.0", "6.4.0"], { verified: false }),
  pkg("Newtonsoft.Json", "13.0.3", "Json.NET is a popular high-performance JSON framework for .NET", "James Newton-King", 4_567_890_123, ["12.0.3", "13.0.1", "13.0.3"]),
];

registerMock("nuget_search", async ({ query }): Promise<SearchResult> => {
  const q = String(query ?? "").toLowerCase();
  const data = FEED.filter((p) => p.id.toLowerCase().includes(q) || p.description.toLowerCase().includes(q));
  return call<SearchResult>("nuget_parse_search", { text: JSON.stringify({ totalHits: data.length * 137, data }) });
});

const versionsOf = (id: string) => FEED.find((p) => p.id.toLowerCase() === id.toLowerCase())?.versions.map((v) => v.version) ?? [];
registerMock("nuget_versions", ({ id }) => versionsOf(String(id)));

registerMock("nuget_updates", async ({ prerelease }): Promise<PackageUpdate[]> => {
  const ws = await loadDemoWorkspace(mockFiles);
  const seen = new Set<string>();
  const out: PackageUpdate[] = [];
  for (const p of ws.projects)
    for (const r of p.packages) {
      const key = `${r.id}@${r.version}`;
      if (!r.version || seen.has(key)) continue;
      seen.add(key);
      const latest = await call<string | null>("nuget_upgrade", { installed: r.version, versions: versionsOf(r.id), prerelease: !!prerelease || !!mockSettingValue("dotnet.nuget.prerelease") });
      if (latest) out.push({ id: r.id, installed: r.version, latest });
    }
  return out;
});

/** `dotnet add|remove package` on the in-memory demo files (central package management). */
function applyPackage(action: string, project: string, id: string, version?: string): void {
  const text = mockFiles[project];
  if (text === undefined) return;
  const central = "Directory.Packages.props";
  const ref = new RegExp(`\\s*<PackageReference Include="${id.replace(/\./g, "\\.")}"[^>]*/>`, "i");
  if (action === "remove") {
    mockFiles[project] = text.replace(ref, "");
    return;
  }
  const v = version || versionsOf(id).at(-1) || "1.0.0";
  if (mockFiles[central]?.includes("ManagePackageVersionsCentrally")) {
    const pv = new RegExp(`<PackageVersion Include="${id.replace(/\./g, "\\.")}" Version="[^"]*" />`, "i");
    const entry = `<PackageVersion Include="${id}" Version="${v}" />`;
    mockFiles[central] = pv.test(mockFiles[central]!) ? mockFiles[central]!.replace(pv, entry) : mockFiles[central]!.replace(/(\s*)<\/ItemGroup>/, `$1  ${entry}$1</ItemGroup>`);
    if (!ref.test(text)) mockFiles[project] = text.replace(/(\s*)<PackageReference /, `$1<PackageReference Include="${id}" />$1<PackageReference `);
    return;
  }
  const entry = `<PackageReference Include="${id}" Version="${v}" />`;
  mockFiles[project] = ref.test(text) ? text.replace(ref, (m) => m.replace(/<PackageReference[^>]*\/>/, entry)) : text.replace(/<\/Project>/, `  <ItemGroup>\n    ${entry}\n  </ItemGroup>\n\n</Project>`);
}

registerMock("nuget_command", async ({ action, project, package: id, version, prerelease }) => {
  const c = await call<CommandLine>("dotnet_package_command", { action, project, package: id, version, prerelease: !!prerelease, context: await dotnetContext(mockFiles) });
  applyPackage(String(action), String(project), String(id), version ? String(version) : undefined);
  return toCommandLine(c);
});

// ---- dotnet format --------------------------------------------------------------------------------

registerMock("dotnet_format", () => null);
registerMock("dotnet_cleanup_command", async ({ paths }) => {
  const scope = String(mockSettingValue("dotnet.cleanup.scope") ?? "all");
  const severity = scope === "whitespace" ? "" : String(mockSettingValue("dotnet.cleanup.severity") ?? "warn");
  const c = await call<CommandLine>("dotnet_format_command", {
    request: { scope, include: paths ?? [], severity, report: "/tmp/forge-dotnet-cleanup-1", verify: false, folder: false, target: "" },
    context: await dotnetContext(mockFiles),
  });
  return { command: toCommandLine(c), reportDir: "/tmp/forge-dotnet-cleanup-1" };
});
registerMock("dotnet_format_report", () => [
  { path: "src/Bookstore.Api/Program.cs", line: 0, column: 0, diagnostic: "IMPORTS", description: "Fix imports ordering." },
  { path: "src/Bookstore.Api/Program.cs", line: 11, column: 20, diagnostic: "WHITESPACE", description: "Fix whitespace formatting. Insert '\\s'." },
]);

// ---- Debugging ------------------------------------------------------------------------------------

registerMock("dotnet_debug_request", ({ configuration }): LaunchRequest => {
  const cfg = configuration as RunConfiguration;
  const api = (cfg.options?.project ?? "").includes("Api");
  return {
    mode: "launch",
    adapter: "coreclr",
    name: cfg.name,
    program: DOTNET,
    args: [api ? `${DOTNET_ROOT}/src/Bookstore.Api/bin/Debug/net8.0/Bookstore.Api.dll` : `${DOTNET_ROOT}/tools/Reports/bin/Debug/net8.0/Reports.dll`],
    cwd: api ? `${DOTNET_ROOT}/src/Bookstore.Api` : `${DOTNET_ROOT}/tools/Reports`,
    env: api ? { ASPNETCORE_ENVIRONMENT: "Development", ASPNETCORE_URLS: "http://localhost:5180" } : {},
    configuration: { justMyCode: true },
    adapterCommand: "/home/anna/.local/share/forge/debuggers/netcoredbg/3.1.2-1054/netcoredbg",
  } as LaunchRequest;
});
registerMock("dotnet_attach_request", ({ pid, name }) => ({ mode: "attachProcess", pid, adapter: "coreclr", name, configuration: { justMyCode: true } }) as unknown as LaunchRequest);

// ---- the demo project ------------------------------------------------------------------------------

if (isDotnetDemo()) {
  registerMock("project_detect", () => structuredClone(dotnetDemoInfo));
  setSetting("project.announcedFrameworks", [`${ROOT}#aspnetcore`], "user");
  if (dotnetBrokenBuild() && mockFiles["src/Bookstore.Api/Program.cs"])
    mockFiles["src/Bookstore.Api/Program.cs"] = mockFiles["src/Bookstore.Api/Program.cs"]!
      .replace("inventory.All())", "inventory.AllBooks())")
      .replace("    inventory.Find(isbn)", "    inventry.Find(isbn)");
  onMockDidOpen((path) => {
    if (!/\.(cs|csx)$/.test(path) || !roslynInstalled) return;
    setMockServerStatus({ id: "roslyn", name: "Roslyn", state: "ready" });
  });
}
