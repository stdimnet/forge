import { describe, expect, it } from "vitest";
import type { BuildProblem, CMakeCompletion, CMakeState, ProfilesInfo } from "@/ipc/cpp";
import type { CommandLine, RunConfiguration } from "@/ipc/run";
import type { TestFile } from "@/ipc/testFrameworks";

// The C++ demo is chosen by the page URL, before the mocks load.
history.replaceState(null, "", "/?project=cpp");
const { invoke } = await import("@/ipc");
const { cppDemoFiles } = await import("./demoCppProject");
await import("./settingsMock");
await import("./testFrameworksMock");
await import("./workspaceMock");
await import("./runMock");
await import("./dapMock");
await import("./cppMock");

describe("CForge mock (Rust crates through WebAssembly)", () => {
  it("discovers GoogleTest and Catch2 tests for the gutter", async () => {
    const gtest = await invoke<TestFile>("test_discover", { path: "tests/polygon_test.cpp", text: cppDemoFiles["tests/polygon_test.cpp"] });
    expect(gtest.framework).toBe("gtest");
    expect(gtest.targets.map((t) => [t.line, t.label])).toEqual([
      [13, "PolygonTest.AreaOfSquare"],
      [17, "PolygonTest.PerimeterOfTriangle"],
      [21, "PolygonTest.ScaledKeepsCorners"],
      [25, "PolygonErrors.NeedsThreeCorners"],
      [31, "ScaleTest.AreaGrowsWithTheSquare"],
    ]);
    const catch2 = await invoke<TestFile>("test_discover", { path: "tests/stats_test.cpp", text: cppDemoFiles["tests/stats_test.cpp"] });
    expect(catch2.framework).toBe("catch2");
    expect(catch2.targets.map((t) => t.key)).toEqual([
      "tests/stats_test.cpp::mean of areas",
      "tests/stats_test.cpp::median",
      "tests/stats_test.cpp::median/odd count",
      "tests/stats_test.cpp::median/even count",
      "tests/stats_test.cpp::empty input throws",
    ]);
  });

  it("builds test command lines with the CMake targets", async () => {
    const cfg: RunConfiguration = { name: "AreaOfSquare", type: "gtest", options: { scope: "test", file: "tests/polygon_test.cpp", suite: "PolygonTest", test: "AreaOfSquare", kind: "test" } };
    const line = await invoke<CommandLine>("run_command_line", { request: { configuration: cfg } });
    expect(line.program).toBe("/home/anna/Projects/shapes/cmake-build-debug/polygon_tests");
    expect(line.args).toEqual(["--gtest_output=xml:/tmp/forge-test-reports/run/gtest-report.xml", "--gtest_filter=PolygonTest.AreaOfSquare"]);
    const rerun = await invoke<RunConfiguration>("test_rerun_failed", { configuration: { ...cfg, type: "catch2" }, failed: [{ name: "median/even count" }] });
    expect(rerun.options?.pattern).toBe("median");
  });

  it("reads profiles, the CMake model and completes CMakeLists.txt", async () => {
    const profiles = await invoke<ProfilesInfo>("cmake_profiles");
    expect(profiles.profiles.map((p) => [p.name, p.preset ?? null])).toEqual([
      ["Debug", null],
      ["Release (preset)", "release"],
      ["ASan (preset)", "asan"],
    ]);
    const state = await invoke<CMakeState>("cmake_state");
    expect(state.status).toBe("loaded");
    expect(state.messages.map((m) => [m.severity, m.file, m.line])).toEqual([["deprecation", "CMakeLists.txt", 23]]);
    const text = "add_executable(app main.cpp)\ntarget_link_libraries(app PRI";
    const done = await invoke<CMakeCompletion>("cmake_complete", { text, offset: text.length });
    expect(done.items.map((i) => i.label)).toEqual(["PRIVATE"]);
  });

  it("parses build output into problems with project paths", async () => {
    const output = "/home/anna/Projects/shapes/src/stats.cpp:21:51: error: no matching function for call to 'midpoint(double&)'\n/usr/include/c++/13/numeric:228:5: note: candidate expects 2 arguments, 1 provided\n";
    const problems = await invoke<BuildProblem[]>("cpp_build_problems", { output });
    expect(problems).toHaveLength(1);
    expect(problems[0]).toMatchObject({ path: "src/stats.cpp", line: 21, column: 51, severity: "error" });
    expect(problems[0].notes?.[0].path).toBe("/usr/include/c++/13/numeric");
  });
});
