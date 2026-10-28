/**
 * The .NET demo's context and canned `dotnet build` / `dotnet run` / `dotnet test` output,
 * without side effects (imported by the test framework mock, the run mock and
 * dotnetMock.ts). Test runs end with the TRX report `dotnet test --logger trx` writes, which
 * the WebAssembly test tree reads like the desktop app.
 */
import type { RunConfiguration } from "@/ipc/run";
import type { DotnetWorkspace } from "@/ipc/dotnet";
import { callWasm, forgeWasm } from "@/wasm";
import { dotnetBrokenBuild, dotnetDemoFiles } from "./demoDotnetProject";
import type { Step } from "./runMock";
import { mockSettingValue } from "./settingsMock";

export const DOTNET_ROOT = "/home/anna/Projects/Bookstore";
export const DOTNET = "/usr/share/dotnet/dotnet";

const PROJECT_FILE = /\.(sln|slnx|csproj|fsproj|vbproj)$|(^|\/)(Directory\.Build\.props|Directory\.Packages\.props|global\.json|launchSettings\.json)$/;

/** The demo files a workspace load reads. */
export function projectFiles(files: Record<string, string> = dotnetDemoFiles): Record<string, string> {
  return Object.fromEntries(Object.entries(files).filter(([p]) => PROJECT_FILE.test(p)));
}

export async function loadDemoWorkspace(files?: Record<string, string>): Promise<DotnetWorkspace> {
  const wasm = await forgeWasm();
  return callWasm<DotnetWorkspace>(wasm.dotnet_load_workspace, {
    files: projectFiles(files),
    solution: String(mockSettingValue("dotnet.solution") ?? ""),
    configuration: String(mockSettingValue("dotnet.configuration") ?? "Debug"),
  });
}

/** The project's .NET context as `commands::dotnet::context` builds it. */
export async function dotnetContext(files?: Record<string, string>) {
  const ws = await loadDemoWorkspace(files);
  return {
    dotnet: DOTNET,
    configuration: String(mockSettingValue("dotnet.configuration") ?? "Debug"),
    verbosity: String(mockSettingValue("dotnet.build.verbosity") ?? "minimal"),
    env: {},
    solution: ws.solution ?? undefined,
    projects: ws.projects.map((p) => ({
      name: p.name,
      path: p.path,
      frameworks: p.targetFrameworks,
      testFramework: p.testFramework ?? undefined,
      test: p.isTestProject,
      runnable: !p.isTestProject && (p.web || /exe$/i.test(p.outputType)),
    })),
    classes: {
      "Bookstore.Tests.CartTests": "tests/Bookstore.Tests/CartTests.cs",
      "Bookstore.Tests.CartTests+WhenEmpty": "tests/Bookstore.Tests/CartTests.cs",
      "Bookstore.Tests.InventoryTests": "tests/Bookstore.Tests/InventoryTests.cs",
    } as Record<string, string>,
    testingPlatform: false,
    windows: false,
  };
}

// ---- dotnet build ------------------------------------------------------------------------------

const R = DOTNET_ROOT;

const BUILD_OK = `  Determining projects to restore...
  All projects are up-to-date for restore.
  Bookstore.Core -> ${R}/src/Bookstore.Core/bin/Debug/netstandard2.0/Acme.Bookstore.Core.dll
  Bookstore.Core -> ${R}/src/Bookstore.Core/bin/Debug/net8.0/Acme.Bookstore.Core.dll
  Reports -> ${R}/tools/Reports/bin/Debug/net8.0/Reports.dll
  Bookstore.Tests -> ${R}/tests/Bookstore.Tests/bin/Debug/net8.0/Bookstore.Tests.dll
  Bookstore.Api -> ${R}/src/Bookstore.Api/bin/Debug/net8.0/Bookstore.Api.dll

Build succeeded.
    0 Warning(s)
    0 Error(s)

Time Elapsed 00:00:04.87
`;

const CS8618 = `${R}/src/Bookstore.Core/Catalog/Cart.cs(5,58): warning CS8618: Non-nullable field '_lines' must contain a non-null value when exiting constructor. Consider adding the 'required' modifier or declaring the field as nullable. [${R}/src/Bookstore.Core/Bookstore.Core.csproj::TargetFramework=net8.0]`;
const CS0103 = `${R}/src/Bookstore.Api/Program.cs(14,5): error CS0103: The name 'inventry' does not exist in the current context [${R}/src/Bookstore.Api/Bookstore.Api.csproj]`;
const CS1061 = `${R}/src/Bookstore.Api/Program.cs(12,57): error CS1061: 'Inventory' does not contain a definition for 'AllBooks' and no accessible extension method 'AllBooks' accepting a first argument of type 'Inventory' could be found (are you missing a using directive or an assembly reference?) [${R}/src/Bookstore.Api/Bookstore.Api.csproj]`;
const NU1603 = `${R}/tests/Bookstore.Tests/Bookstore.Tests.csproj : warning NU1603: Bookstore.Tests depends on xunit.runner.visualstudio (>= 2.5.7) but xunit.runner.visualstudio 2.5.7 was not found. An approximate best match of xunit.runner.visualstudio 2.5.8 was resolved. [${R}/Bookstore.sln]`;

const BUILD_BROKEN = `  Determining projects to restore...
${NU1603}
  Restored ${R}/tests/Bookstore.Tests/Bookstore.Tests.csproj (in 412 ms).
${CS8618}
  Bookstore.Core -> ${R}/src/Bookstore.Core/bin/Debug/net8.0/Acme.Bookstore.Core.dll
${CS1061}
${CS0103}

Build FAILED.

${NU1603}
${CS8618}
${CS1061}
${CS0103}
    2 Warning(s)
    2 Error(s)

Time Elapsed 00:00:03.71
`;

const lines = (text: string, delay = 60): Step[] => text.split(/(?<=\n)/).map((l): Step => [delay, l]);

/** Canned output of `dotnet build` / `clean` / `restore` and NuGet commands. */
export function dotnetCommandSteps(args: string[]): Step[] {
  const sub = args[0] ?? "build";
  if (sub === "clean") return [...lines(`  Bookstore.Core -> cleaned\n\nBuild succeeded.\n    0 Warning(s)\n    0 Error(s)\n\nTime Elapsed 00:00:00.91\n`, 40), [10, { code: 0 }]];
  if (sub === "restore") return [...lines("  Determining projects to restore...\n  All projects are up-to-date for restore.\n", 200), [10, { code: 0 }]];
  if (sub === "add" || sub === "remove") {
    const project = args[1] ?? "";
    const pkg = args[3] ?? "";
    const text =
      sub === "add"
        ? `  Determining projects to restore...\n  Writing /tmp/tmpk3Lq0b.tmp\ninfo : Adding PackageReference for package '${pkg}' into project '${R}/${project}'.\ninfo : Restoring packages for ${R}/${project}...\ninfo : Package '${pkg}' is compatible with all the specified frameworks in project '${R}/${project}'.\ninfo : PackageReference for package '${pkg}' added to file '${R}/${project}'.\nlog  : Restored ${R}/${project} (in 318 ms).\n`
        : `info : Removing PackageReference for package '${pkg}' from project '${R}/${project}'.\n`;
    return [...lines(text, 120), [10, { code: 0 }]];
  }
  if (sub === "new") return [...lines(`The template "${args[1]}" was created successfully.\n\nProcessing post-creation actions...\nRestoring ${R}/${args[5] ?? ""}:\nRestore succeeded.\n`, 150), [10, { code: 0 }]];
  if (sub === "sln") return [...lines(`Project \`${args[3] ?? ""}\` added to the solution.\n`, 80), [10, { code: 0 }]];
  if (sub === "format") return [...lines("  Formatted code file 'Program.cs'.\n  Format complete in 2841ms.\n", 400), [10, { code: 0 }]];
  const broken = dotnetBrokenBuild();
  return [...lines(broken ? BUILD_BROKEN : BUILD_OK, 90), [10, { code: broken ? 1 : 0 }]];
}

// ---- dotnet run -----------------------------------------------------------------------------------

export function dotnetRunSteps(cfg: RunConfiguration): Step[] {
  const o = cfg.options ?? {};
  const project = o.project ?? "";
  if (project.includes("Reports")) return [[1200, "Revenue: 29.97\n"], [10, { code: 0 }]];
  const watch = o.mode === "watch";
  const profile = o.profile || "http";
  const url = profile === "https" ? "https://localhost:7180" : "http://localhost:5180";
  const out: Step[] = [];
  if (watch)
    out.push(
      [300, "dotnet watch 🔥 Hot reload enabled. For a list of supported edits, see https://aka.ms/dotnet/hot-reload.\n"],
      [20, '  💡 Press "Ctrl + R" to restart.\n'],
      [20, "dotnet watch 🔨 Building...\n"],
    );
  else out.push([600, "Building...\n"]);
  out.push(
    [1400, "info: Microsoft.Hosting.Lifetime[14]\n"],
    [10, `      Now listening on: ${url}\n`],
    [10, "info: Microsoft.Hosting.Lifetime[0]\n"],
    [10, "      Application started. Press Ctrl+C to shut down.\n"],
    [10, "info: Microsoft.Hosting.Lifetime[0]\n"],
    [10, "      Hosting environment: Development\n"],
    [10, "info: Microsoft.Hosting.Lifetime[0]\n"],
    [10, `      Content root path: ${R}/src/Bookstore.Api\n`],
  );
  if (watch) out.push([10, "dotnet watch 🚀 Started\n"]);
  return out;
}

// ---- dotnet test ----------------------------------------------------------------------------------

interface DemoTest {
  cls: string;
  method: string;
  display: string;
  outcome: "Passed" | "Failed" | "NotExecuted";
  ms: number;
  message?: string;
  trace?: string;
}

const TESTS: DemoTest[] = [
  { cls: "Bookstore.Tests.CartTests", method: "Total_IsZero_ForAnEmptyCart", display: "Total_IsZero_ForAnEmptyCart", outcome: "Passed", ms: 3 },
  {
    cls: "Bookstore.Tests.CartTests",
    method: "Total_AddsLines",
    display: "Total_AddsLines",
    outcome: "Failed",
    ms: 12,
    message: "Expected cart.Total to be 19.98M, but found 19.97M.",
    trace: `   at FluentAssertions.Execution.LateBoundTestFramework.Throw(String message)\n   at Bookstore.Tests.CartTests.Total_AddsLines() in ${R}/tests/Bookstore.Tests/CartTests.cs:line 22`,
  },
  { cls: "Bookstore.Tests.CartTests", method: "Add_RejectsNonPositiveQuantities", display: "Add_RejectsNonPositiveQuantities(quantity: 0)", outcome: "Passed", ms: 5 },
  { cls: "Bookstore.Tests.CartTests", method: "Add_RejectsNonPositiveQuantities", display: "Add_RejectsNonPositiveQuantities(quantity: -1)", outcome: "Passed", ms: 1 },
  { cls: "Bookstore.Tests.CartTests", method: "Total_AppliesVouchers", display: "Total_AppliesVouchers", outcome: "NotExecuted", ms: 0, message: "Discounts are not implemented yet" },
  { cls: "Bookstore.Tests.CartTests+WhenEmpty", method: "HasNoLines", display: "HasNoLines", outcome: "Passed", ms: 1 },
  { cls: "Bookstore.Tests.InventoryTests", method: "Find_ReturnsAddedBooks", display: "Find_ReturnsAddedBooks", outcome: "Passed", ms: 2 },
  { cls: "Bookstore.Tests.InventoryTests", method: "Find_ReturnsNull_ForUnknownIsbn", display: "Find_ReturnsNull_ForUnknownIsbn", outcome: "Passed", ms: 1 },
];

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** The tests a configuration selects (its scope, class, method or the rerun's names). */
function selected(cfg: RunConfiguration): DemoTest[] {
  const o = cfg.options ?? {};
  switch (o.scope || "all") {
    case "class":
      return TESTS.filter((t) => t.cls === o.class || t.cls.startsWith(`${o.class}+`));
    case "method":
      return TESTS.filter((t) => t.cls === o.class && t.method === o.method);
    case "filter": {
      const rerun = (o.tests ?? "").split("\n").map((l) => l.split("::").pop()!.trim()).filter(Boolean);
      if (rerun.length) return TESTS.filter((t) => rerun.includes(t.method));
      const parts = (o.filter ?? "").split("|").map((p) => p.replace(/^FullyQualifiedName[~=]/, "").replace(/\\/g, ""));
      return TESTS.filter((t) => parts.some((p) => `${t.cls}.${t.method}`.includes(p.replace(/\($/, "")) || t.cls.includes(p.replace(/[.+]$/, ""))));
    }
    default:
      return TESTS;
  }
}

function trx(tests: DemoTest[]): string {
  const id = (i: number) => `7000000${i}-0000-0000-0000-000000000000`;
  const results = tests
    .map((t, i) => {
      const dur = `00:00:00.${String(t.ms).padStart(3, "0")}0000`;
      const head = `<UnitTestResult executionId="e${i}" testId="${id(i)}" testName="${esc(`${t.cls}.${t.display}`)}" computerName="forge-dev" duration="${dur}" outcome="${t.outcome}"`;
      if (!t.message) return `    ${head} />`;
      return `    ${head}>\n      <Output><ErrorInfo><Message>${esc(t.message)}</Message>${t.trace ? `<StackTrace>${esc(t.trace)}</StackTrace>` : ""}</ErrorInfo></Output>\n    </UnitTestResult>`;
    })
    .join("\n");
  const defs = tests
    .map((t, i) => `    <UnitTest name="${esc(`${t.cls}.${t.display}`)}" id="${id(i)}"><TestMethod codeBase="${R}/tests/Bookstore.Tests/bin/Debug/net8.0/Bookstore.Tests.dll" adapterTypeName="executor://xunit/VsTestRunner2/netcoreapp" className="${t.cls}" name="${t.method}" /></UnitTest>`)
    .join("\n");
  return `<?xml version="1.0" encoding="utf-8"?>\n<TestRun id="5c1f6a8e-1b2c-4d3e-8f90-0a1b2c3d4e5f" name="anna@forge-dev" runUser="anna" xmlns="http://microsoft.com/schemas/VisualStudio/TeamTest/2010">\n  <Results>\n${results}\n  </Results>\n  <TestDefinitions>\n${defs}\n  </TestDefinitions>\n</TestRun>\n`;
}

/** Canned `dotnet test` output (console logger) and the TRX report. */
export function dotnetTestSteps(cfg: RunConfiguration): Step[] {
  const tests = selected(cfg);
  const failed = tests.filter((t) => t.outcome === "Failed").length;
  const skipped = tests.filter((t) => t.outcome === "NotExecuted").length;
  const passed = tests.length - failed - skipped;
  const steps: Step[] = [
    [200, "  Determining projects to restore...\n"],
    [300, "  All projects are up-to-date for restore.\n"],
    [600, `  Bookstore.Core -> ${R}/src/Bookstore.Core/bin/Debug/net8.0/Acme.Bookstore.Core.dll\n`],
    [500, `  Bookstore.Tests -> ${R}/tests/Bookstore.Tests/bin/Debug/net8.0/Bookstore.Tests.dll\n`],
    [100, `Test run for ${R}/tests/Bookstore.Tests/bin/Debug/net8.0/Bookstore.Tests.dll (.NETCoreApp,Version=v8.0)\n`],
    [20, "VSTest version 17.11.1 (x64)\n\n"],
    [20, "Starting test execution, please wait...\n"],
    [400, "A total of 1 test files matched the specified pattern.\n"],
  ];
  for (const t of tests) {
    if (t.outcome === "NotExecuted") steps.push([20, `  Skipped ${t.cls}.${t.display} [< 1 ms]\n`]);
    else if (t.outcome === "Failed") steps.push([t.ms * 10, `  Failed ${t.cls}.${t.display} [${t.ms} ms]\n  Error Message:\n   ${t.message}\n  Stack Trace:\n${t.trace}\n\n`]);
    else steps.push([t.ms * 10, `  Passed ${t.cls}.${t.display} [${t.ms} ms]\n`]);
  }
  steps.push(
    [40, `\nResults File: /tmp/forge-test-reports/run/forge_net8.0_20261028091403.trx\n\n`],
    [20, `${failed ? "Failed!" : "Passed!"}  - Failed:     ${failed}, Passed:     ${passed}, Skipped:     ${skipped}, Total:     ${tests.length}, Duration: 48 ms - Bookstore.Tests.dll (net8.0)\n`],
    [10, { report: trx(tests) }],
    [10, { code: failed ? 1 : 0 }],
  );
  return steps;
}
