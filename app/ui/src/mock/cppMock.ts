/**
 * Browser-mode CForge: the `cmake_*` / `cpp_*` commands and C++ runs on the demo CMake
 * project (`?project=cpp`, demoCppProject.ts). Profiles and presets, CMakeLists completion,
 * configure log messages, build problems, test discovery and the GoogleTest / Catch2 / CTest
 * command lines go through the Rust CForge crates (WebAssembly), like in the desktop app; the
 * toolchains, the CMake code model, the configure and build output and the test reports are
 * canned. Nothing is configured, built or run.
 */
import { emitMock, registerMock } from "@/ipc";
import type {
  BuildProblem,
  CMakeModel,
  CMakeState,
  CMakeTarget,
  ClangdInfo,
  CppStatus,
  ProfileEntry,
  ProfilesInfo,
  Toolchain,
  ToolchainEntry,
} from "@/ipc/cpp";
import type { CommandLine, RunConfiguration, RunEvent, RunRequest } from "@/ipc/run";
import type { LaunchRequest } from "@/features/debug/dapApi";
import { callWasmSync, forgeWasm } from "@/wasm";
import { cppBrokenBuild, cppDemoInfo, cppDemoName, isCppDemo, isPioDemo } from "./demoCppProject";
import { mockFiles } from "./workspaceMock";
import { mockSettingValue, mockTestFrameworkSpecs } from "./settingsMock";
import { registerMockRunHandler, startMockRun, type Step } from "./runMock";
import "./cppDebugMock";
import { asanOutput, nativeRun, sanitizerEvent } from "./cppToolsMock";
import "./embeddedMock";

const HOME = "/home/anna";
export const CPP_ROOT = `${HOME}/Projects/${cppDemoName}`;
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

if (isCppDemo()) {
  registerMock("project_detect", () => structuredClone(cppDemoInfo));
}

// ---- Toolchains -------------------------------------------------------------------------------

const TOOLS = {
  cmake: { path: "/usr/bin/cmake", version: "3.28.3" },
  ninja: { path: "/usr/bin/ninja", version: "1.11.1" },
  make: { path: "/usr/bin/make", version: "4.3" },
};

const TOOLCHAINS: Toolchain[] = [
  {
    id: "system-gcc",
    name: "GCC 13.3.0",
    kind: "system",
    cCompiler: "/usr/bin/gcc",
    cxxCompiler: "/usr/bin/g++",
    compiler: { family: "gcc", version: "13.3.0", target: "x86_64-linux-gnu", threadModel: "posix", vendor: "Ubuntu" },
    ...TOOLS,
    debugger: { kind: "gdb", path: "/usr/bin/gdb", version: "15.1" },
    source: "detected",
  },
  {
    id: "system-clang",
    name: "Clang 18.1.3",
    kind: "system",
    cCompiler: "/usr/bin/clang",
    cxxCompiler: "/usr/bin/clang++",
    compiler: { family: "clang", version: "18.1.3", target: "x86_64-pc-linux-gnu", threadModel: "posix", installedDir: "/usr/bin", vendor: "Ubuntu" },
    ...TOOLS,
    debugger: { kind: "lldb", path: "/usr/bin/lldb-dap", version: "18.1.3" },
    source: "detected",
  },
  {
    id: "cross-arm-none-eabi",
    name: "GCC 13.3.1 (arm-none-eabi)",
    kind: "cross",
    cCompiler: "/opt/arm-gnu-toolchain/bin/arm-none-eabi-gcc",
    cxxCompiler: "/opt/arm-gnu-toolchain/bin/arm-none-eabi-g++",
    compiler: { family: "gcc", version: "13.3.1", target: "arm-none-eabi", threadModel: "single", vendor: "Arm GNU Toolchain 13.3.Rel1" },
    ...TOOLS,
    debugger: { kind: "gdb", path: "/usr/bin/gdb-multiarch", version: "15.1" },
    source: "detected",
  },
];

const setting = <T>(key: string, fallback: T): T => (mockSettingValue(key) as T | undefined) ?? fallback;

function fromEntry(e: ToolchainEntry): Toolchain {
  const gcc = /g\+\+|gcc/.test(e.cxxCompiler || e.cCompiler);
  return {
    id: e.name,
    name: e.name,
    kind: e.kind === "mingw" ? "minGw" : ((e.kind || "custom") as Toolchain["kind"]),
    cCompiler: e.cCompiler || undefined,
    cxxCompiler: e.cxxCompiler || undefined,
    compiler: e.cxxCompiler ? { family: gcc ? "gcc" : "clang", version: gcc ? "13.3.0" : "18.1.3", target: "x86_64-linux-gnu" } : undefined,
    cmake: e.cmake ? { path: e.cmake, version: "3.28.3" } : TOOLS.cmake,
    ninja: e.ninja ? { path: e.ninja, version: "1.11.1" } : TOOLS.ninja,
    make: e.make ? { path: e.make, version: "4.3" } : TOOLS.make,
    debugger: e.debugger ? { kind: e.debugger.includes("lldb") ? "lldb" : "gdb", path: e.debugger, version: "15.1" } : { kind: "gdb", path: "/usr/bin/gdb", version: "15.1" },
    environmentScript: e.environmentScript || undefined,
    source: "settings",
    problems: e.cxxCompiler ? [] : ["No C++ compiler"],
  };
}

const allToolchains = (): Toolchain[] => [...setting<ToolchainEntry[]>("cpp.toolchains", []).filter((e) => e.name).map(fromEntry), ...TOOLCHAINS];

registerMock("cpp_toolchains", () => structuredClone(allToolchains()));
registerMock("cpp_toolchain_inspect", ({ entry }) => fromEntry(entry as ToolchainEntry));

// ---- Profiles ---------------------------------------------------------------------------------

async function profilesInfo(): Promise<ProfilesInfo> {
  const wasm = await forgeWasm();
  const files: Record<string, string> = {};
  for (const f of ["CMakePresets.json", "CMakeUserPresets.json"]) if (mockFiles[f]) files[f] = mockFiles[f];
  const out = callWasmSync<Omit<ProfilesInfo, "active">>(wasm.cmake_profiles, {
    entries: setting<ProfileEntry[]>("cmake.profiles", []),
    files,
    root: CPP_ROOT,
  });
  const wanted = String(setting("cmake.activeProfile", ""));
  const active = out.profiles.find((p) => p.name === wanted && p.enabled) ?? out.profiles.find((p) => p.enabled) ?? out.profiles[0];
  return { ...out, active: active?.name ?? "Debug" };
}

registerMock("cmake_profiles", () => profilesInfo());

// ---- The CMake model ----------------------------------------------------------------------------

const target = (name: string, kind: CMakeTarget["kind"], line: number, sources: string[], artifact: string, dependencies: string[] = []): CMakeTarget => ({
  name,
  id: `${name}::@6890427a1f51a3e7e1df`,
  kind,
  artifacts: artifact ? [artifact] : [],
  sourceDir: "",
  buildDir: "",
  sources: sources.map((path) => ({ path, language: path.endsWith(".hpp") ? undefined : "CXX" })),
  dependencies,
  definedAt: { file: "CMakeLists.txt", line, command: kind === "executable" ? "add_executable" : "add_library" },
  project: "shapes",
});

function model(buildDir: string, buildType: string): CMakeModel {
  const t = (name: string, kind: CMakeTarget["kind"], line: number, sources: string[], artifact: string, deps: string[] = []) => ({
    ...target(name, kind, line, sources, artifact ? `${buildDir}/${artifact}` : "", deps),
    buildDir,
  });
  return {
    cmakeVersion: "3.28.3",
    generator: "Ninja",
    multiConfig: false,
    sourceDir: CPP_ROOT,
    buildDir,
    configurations: [
      {
        name: buildType,
        projects: ["shapes", "googletest-distribution", "Catch2"],
        targets: [
          t("shapes", "staticLibrary", 10, ["src/geometry/polygon.cpp", "src/geometry/polygon.hpp", "src/stats.cpp", "src/stats.hpp"], "libshapes.a"),
          t("shapes_app", "executable", 18, ["src/main.cpp"], "shapes_app", ["shapes"]),
          t("polygon_tests", "executable", 34, ["tests/polygon_test.cpp"], "polygon_tests", ["shapes", "gtest_main"]),
          t("stats_tests", "executable", 38, ["tests/stats_test.cpp"], "stats_tests", ["shapes", "Catch2WithMain"]),
          { ...t("gtest", "staticLibrary", 0, [], "lib/libgtest.a"), project: "googletest-distribution", definedAt: undefined },
          { ...t("gtest_main", "staticLibrary", 0, [], "lib/libgtest_main.a"), project: "googletest-distribution", definedAt: undefined },
          { ...t("Catch2WithMain", "staticLibrary", 0, [], "_deps/catch2-build/src/libCatch2Maind.a"), project: "Catch2", definedAt: undefined },
        ],
      },
    ],
    toolchains: [{ language: "CXX", compilerId: "GNU", path: "/usr/bin/g++", version: "13.3.0" }],
    cmakeFiles: ["CMakeLists.txt", "CMakePresets.json"],
  };
}

function configureLog(buildDir: string, buildType: string, generator: string): string[] {
  return [
    `/usr/bin/cmake -DCMAKE_BUILD_TYPE=${buildType} -DCMAKE_MAKE_PROGRAM=/usr/bin/ninja -DCMAKE_C_COMPILER=/usr/bin/gcc -DCMAKE_CXX_COMPILER=/usr/bin/g++ -G ${generator} -S ${CPP_ROOT} -B ${buildDir}`,
    "-- The CXX compiler identification is GNU 13.3.0",
    "-- Detecting CXX compiler ABI info",
    "-- Detecting CXX compiler ABI info - done",
    "-- Check for working CXX compiler: /usr/bin/g++ - skipped",
    "-- Detecting CXX compile features",
    "-- Detecting CXX compile features - done",
    "CMake Warning (dev) at CMakeLists.txt:23 (FetchContent_Declare):",
    "  The DOWNLOAD_EXTRACT_TIMESTAMP option was not given and policy CMP0135 is",
    "  not set.  The policy's OLD behavior will be used.  When using a URL",
    "  download, the timestamps of extracted files should preferably be that of",
    "  the time of extraction, otherwise code that depends on the extracted",
    "  contents might not be rebuilt if the URL changes.",
    "This warning is for project developers.  Use -Wno-dev to suppress it.",
    "",
    "-- Found Python3: /usr/bin/python3 (found version \"3.12.3\") found components: Interpreter",
    "-- Performing Test CMAKE_HAVE_LIBC_PTHREAD",
    "-- Performing Test CMAKE_HAVE_LIBC_PTHREAD - Success",
    "-- Found Threads: TRUE",
    "-- Configuring done (2.4s)",
    "-- Generating done (0.1s)",
    `-- Build files have been written to: ${buildDir}`,
  ];
}

let state: CMakeState | null = null;

async function currentState(): Promise<CMakeState> {
  if (!mockFiles["CMakeLists.txt"]) return { root: CPP_ROOT, profile: "Debug", buildDir: `${CPP_ROOT}/cmake-build-debug`, status: "noCMake", messages: [], log: [] };
  const info = await profilesInfo();
  const p = info.profiles.find((x) => x.name === info.active) ?? info.profiles[0];
  const dir = p.buildDirectory || `cmake-build-${p.name.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`;
  const buildDir = dir.startsWith("/") ? dir : `${CPP_ROOT}/${dir}`;
  if (state && state.profile === p.name && state.buildDir === buildDir) return state;
  // The Debug profiles were loaded when the project opened; other profiles need a reload.
  const loaded = p.name === "Debug" || p.name === "Debug-ASan";
  const log = loaded ? configureLog(buildDir, p.buildType, "Ninja") : [];
  const wasm = await forgeWasm();
  state = {
    root: CPP_ROOT,
    profile: p.name,
    buildDir,
    status: loaded ? "loaded" : "notLoaded",
    model: loaded ? model(buildDir, p.buildType) : undefined,
    messages: loaded ? callWasmSync(wasm.cmake_parse_log, { text: log.join("\n") }) : [],
    log,
  };
  return state;
}

registerMock("cmake_state", () => currentState());

registerMock("cmake_reload", async () => {
  const s = await currentState();
  const info = await profilesInfo();
  const p = info.profiles.find((x) => x.name === s.profile)!;
  const log = configureLog(s.buildDir, p.buildType, p.generator || "Ninja");
  state = { ...s, status: "loading", model: undefined, messages: [], log: [] };
  emitMock("cmake://state", state);
  for (const line of log) {
    await sleep(25);
    emitMock("cmake://log", { line, stream: line.startsWith("CMake ") || line.startsWith("  ") ? "stderr" : "stdout" });
  }
  const wasm = await forgeWasm();
  state = { ...s, status: "loaded", model: model(s.buildDir, p.buildType), messages: callWasmSync(wasm.cmake_parse_log, { text: log.join("\n") }), log };
  emitMock("cmake://state", state);
  return state;
});
registerMock("cmake_cancel", () => undefined);

registerMock("cmake_build_command", async ({ action, targets }) => {
  const s = await currentState();
  const args = ["--build", s.buildDir];
  if (action === "clean") args.push("--target", "clean");
  else {
    const list = (targets as string[] | undefined) ?? [];
    if (list.length) args.push("--target", ...list);
    if (action === "rebuild") args.push("--clean-first");
  }
  args.push("-j", "8");
  return { program: "/usr/bin/cmake", args, cwd: CPP_ROOT, env: {} } satisfies CommandLine;
});

registerMock("cpp_build_problems", async ({ output }) => {
  const s = await currentState();
  const wasm = await forgeWasm();
  return callWasmSync<BuildProblem[]>(wasm.cpp_build_problems, {
    output,
    root: CPP_ROOT,
    buildDir: s.buildDir.slice(CPP_ROOT.length + 1),
    files: Object.keys(mockFiles),
  });
});

registerMock("cmake_complete", async ({ text, offset }) => callWasmSync((await forgeWasm()).cmake_complete, { text, offset }));

registerMock("cpp_target_of", async ({ configuration }) => {
  const cfg = configuration as RunConfiguration;
  if (cfg.type === "cmake-app") return cfg.options?.target || null;
  if (cfg.type !== "gtest" && cfg.type !== "catch2") return null;
  if (cfg.options?.target) return cfg.options.target;
  const file = cfg.options?.file ?? "";
  return (await cppContext()).targets.find((t) => t.sources.includes(file))?.name ?? null;
});

// ---- clangd, clang-format and status ---------------------------------------------------------

let clangd: ClangdInfo = { path: "/usr/bin/clangd", version: "18.1.3", source: "path", pinnedVersion: "19.1.2" };

registerMock("cpp_status", async (): Promise<CppStatus> => {
  const s = await currentState();
  return {
    clangd,
    clangFormat: "/usr/bin/clang-format",
    toolchain: allToolchains()[0],
    projectKind: s.status === "noCMake" ? "sources" : "cmake",
    compileCommandsDir: s.buildDir,
    compileCommands: { files: 5, projectFiles: 5, compilers: ["/usr/bin/g++"], standards: ["c++20"] },
    remote: false,
  };
});

registerMock("cpp_clangd_install", async () => {
  emitMock("cpp://install", { message: "Reading the SHA-256 of clangd-linux-19.1.2.zip from https://api.github.com/repos/clangd/clangd/releases/tags/19.1.2…" });
  for (let done = 0; done <= 30_000_000; done += 6_000_000) {
    await sleep(80);
    emitMock("cpp://install", { label: "Downloading clangd 19.1.2", done, total: 30_000_000 });
  }
  emitMock("cpp://install", { message: "Verified SHA-256 of clangd 19.1.2." });
  clangd = { path: `${HOME}/.local/share/forge/langservers/clangd/19.1.2/bin/clangd`, version: "19.1.2", source: "managed", pinnedVersion: "19.1.2" };
  return clangd;
});

// clang-format: the demo files are formatted already.
registerMock("cpp_format", ({ content }) => content);

// ---- Runs, builds and tests -------------------------------------------------------------------

async function cppContext() {
  const s = await currentState();
  const targets = (s.model?.configurations[0]?.targets ?? [])
    .filter((t) => t.kind === "executable")
    .map((t) => ({ name: t.name, executable: t.artifacts?.[0] ?? "", sources: t.sources.map((x) => x.path) }));
  return {
    buildDir: s.buildDir,
    buildType: "Debug",
    multiConfig: false,
    ctest: "/usr/bin/ctest",
    targets,
    coverageTools: { gcov: "/usr/bin/gcov", "llvm-profdata": "/usr/bin/llvm-profdata", "llvm-cov": "/usr/bin/llvm-cov" },
  };
}

registerMock("cpp_debug_request", async ({ configuration }): Promise<LaunchRequest> => {
  const cfg = configuration as RunConfiguration;
  const ctx = await cppContext();
  const name = cfg.type === "cmake-app" ? cfg.options?.target : ctx.targets.find((t) => t.sources.includes(cfg.options?.file ?? ""))?.name;
  const program = ctx.targets.find((t) => t.name === name)?.executable ?? `${ctx.buildDir}/shapes_app`;
  const adapter = cfg.options?.debugger || "gdb";
  return {
    mode: "launch",
    adapter,
    name: cfg.name,
    program,
    cwd: ctx.buildDir,
    stopOnEntry: false,
    adapterArgs: adapter === "gdb" ? ["-iex", "set print pretty on", "-iex", "python exec('import sys\\nsys.path.insert(0, \\'/usr/share/gcc/python\\')…')"] : [],
  } as LaunchRequest;
});

const ninja = (lines: string[]) => lines.map((l): Step => [90, `${l}\n`]);

function buildSteps(args: string[]): Step[] {
  if (args.includes("clean")) return [[150, "[1/1] Cleaning all built files...\n"], [50, "Cleaning... 14 files.\n"], [30, { code: 0 }]];
  const broken = cppBrokenBuild();
  const steps: Step[] = ninja([
    "[1/8] Building CXX object CMakeFiles/shapes.dir/src/stats.cpp.o",
    "[2/8] Building CXX object CMakeFiles/shapes.dir/src/geometry/polygon.cpp.o",
  ]);
  if (broken) {
    // Lines of the broken demo files (demoCppProject.ts).
    const lineOf = (file: string, text: string) => (mockFiles[file] ?? "").split("\n").findIndex((l) => l.includes(text)) + 1;
    const colOf = (file: string, text: string, word: string) => ((mockFiles[file] ?? "").split("\n")[lineOf(file, text) - 1]?.indexOf(word) ?? 0) + 1;
    const p = "src/geometry/polygon.cpp";
    const hyp = lineOf(p, "std::hypotenuse");
    const unused = lineOf(p, "std::size_t count");
    const mid = lineOf("src/stats.cpp", "std::midpoint");
    steps.push(
      [120, "FAILED: CMakeFiles/shapes.dir/src/geometry/polygon.cpp.o \n"],
      [10, `/usr/bin/g++  -I${CPP_ROOT}/src -g -std=gnu++20 -Wall -Wextra -MD -MT CMakeFiles/shapes.dir/src/geometry/polygon.cpp.o -MF CMakeFiles/shapes.dir/src/geometry/polygon.cpp.o.d -o CMakeFiles/shapes.dir/src/geometry/polygon.cpp.o -c ${CPP_ROOT}/${p}\n`],
      [10, `${CPP_ROOT}/${p}: In member function 'double shapes::Polygon::perimeter() const':\n`, "stderr"],
      [10, `${CPP_ROOT}/${p}:${hyp}:${colOf(p, "std::hypotenuse", "hypotenuse")}: error: 'hypotenuse' is not a member of 'std'; did you mean 'hypot'?\n`, "stderr"],
      [5, `   ${hyp} |         sum += std::hypotenuse(b.x - a.x, b.y - a.y);\n      |                     ^~~~~~~~~~\n      |                     hypot\n`, "stderr"],
      [10, `${CPP_ROOT}/${p}: In member function 'shapes::Polygon shapes::Polygon::scaled(double) const':\n`, "stderr"],
      [10, `${CPP_ROOT}/${p}:${unused}:${colOf(p, "std::size_t count", "count")}: warning: unused variable 'count' [-Wunused-variable]\n`, "stderr"],
      [5, `   ${unused} |     std::size_t count = corners_.size();\n      |                 ^~~~~\n`, "stderr"],
      [20, "FAILED: CMakeFiles/shapes.dir/src/stats.cpp.o \n"],
      [10, `${CPP_ROOT}/src/stats.cpp: In function 'double shapes::median(std::vector<double>)':\n`, "stderr"],
      [10, `${CPP_ROOT}/src/stats.cpp:${mid}:${colOf("src/stats.cpp", "std::midpoint", "std::midpoint") + 13}: error: no matching function for call to 'midpoint(__gnu_cxx::__alloc_traits<std::allocator<double>, double>::value_type&)'\n`, "stderr"],
      [5, `   ${mid} |     return values.size() % 2 ? values[mid] : std::midpoint(values[mid]);\n      |                                              ~~~~~~~~~~~~~^~~~~~~~~~~~~\n`, "stderr"],
      [5, "/usr/include/c++/13/numeric:228:5: note: candidate: 'template<class _Tp> constexpr std::enable_if_t<...> std::midpoint(_Tp, _Tp)'\n", "stderr"],
      [5, "/usr/include/c++/13/numeric:228:5: note:   candidate expects 2 arguments, 1 provided\n", "stderr"],
      [20, "ninja: build stopped: subcommand failed.\n"],
      [30, { code: 1 }],
    );
    return steps;
  }
  steps.push(
    ...ninja([
      "[3/8] Building CXX object CMakeFiles/shapes_app.dir/src/main.cpp.o",
      "[4/8] Linking CXX static library libshapes.a",
      "[5/8] Building CXX object CMakeFiles/polygon_tests.dir/tests/polygon_test.cpp.o",
      "[6/8] Building CXX object CMakeFiles/stats_tests.dir/tests/stats_test.cpp.o",
      "[7/8] Linking CXX executable shapes_app",
      "[8/8] Linking CXX executable polygon_tests",
    ]),
    [30, { code: 0 }],
  );
  return steps;
}

// GoogleTest and Catch2 report files, the way the frameworks write them.
interface Case {
  suite: string;
  name: string;
  line: number;
  fail?: string;
}
const GTEST: Case[] = [
  { suite: "PolygonTest", name: "AreaOfSquare", line: 14 },
  { suite: "PolygonTest", name: "PerimeterOfTriangle", line: 18 },
  { suite: "PolygonTest", name: "ScaledKeepsCorners", line: 22 },
  { suite: "PolygonErrors", name: "NeedsThreeCorners", line: 26 },
  { suite: "Factors/ScaleTest", name: "AreaGrowsWithTheSquare/0", line: 32 },
  { suite: "Factors/ScaleTest", name: "AreaGrowsWithTheSquare/1", line: 32 },
  { suite: "Factors/ScaleTest", name: "AreaGrowsWithTheSquare/2", line: 32 },
];
const CATCH: Case[] = [
  { suite: "stats_tests.global", name: "mean of areas", line: 8 },
  { suite: "stats_tests.global", name: "median/odd count", line: 13 },
  {
    suite: "stats_tests.global",
    name: "median/even count",
    line: 16,
    fail: `FAILED:\n  REQUIRE( shapes::median({1.0, 2.0, 3.0, 4.0}) == 2.5 )\nwith expansion:\n  3.0 == 2.5\nat ${CPP_ROOT}/tests/stats_test.cpp:17`,
  },
  { suite: "stats_tests.global", name: "empty input throws", line: 21 },
];

const xml = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

function gtestReport(cases: Case[]): string {
  const suites = [...new Set(cases.map((c) => c.suite))];
  return `<?xml version="1.0" encoding="UTF-8"?>\n<testsuites tests="${cases.length}" failures="0" disabled="0" errors="0" time="0.003" name="AllTests">\n${suites
    .map(
      (s) =>
        `  <testsuite name="${s}" tests="${cases.filter((c) => c.suite === s).length}" failures="0" errors="0" time="0.001">\n${cases
          .filter((c) => c.suite === s)
          .map((c) => `    <testcase name="${c.name}" file="${CPP_ROOT}/tests/polygon_test.cpp" line="${c.line}" status="run" result="completed" time="0" classname="${s}" />`)
          .join("\n")}\n  </testsuite>`,
    )
    .join("\n")}\n</testsuites>\n`;
}

function catchReport(cases: Case[]): string {
  return `<?xml version="1.0" encoding="UTF-8"?>\n<testsuites>\n  <testsuite name="stats_tests" errors="0" failures="${cases.filter((c) => c.fail).length}" tests="${cases.length}" time="0.0008">\n${cases
    .map((c) =>
      c.fail
        ? `    <testcase classname="${c.suite}" name="${xml(c.name)}" time="0.0001" status="run">\n      <failure message="${xml(c.fail.split("\n")[1].trim())}" type="REQUIRE">${xml(c.fail)}</failure>\n    </testcase>`
        : `    <testcase classname="${c.suite}" name="${xml(c.name)}" time="0.0001" status="run"/>`,
    )
    .join("\n")}\n  </testsuite>\n</testsuites>\n`;
}

function ctestReport(names: { name: string; fail?: string }[]): string {
  return `<?xml version="1.0" encoding="UTF-8"?>\n<testsuite name="(empty)" tests="${names.length}" failures="${names.filter((n) => n.fail).length}" disabled="0" skipped="0" time="0">\n${names
    .map((n) =>
      n.fail
        ? `\t<testcase name="${xml(n.name)}" classname="${xml(n.name)}" time="0.004" status="fail">\n\t\t<failure message="Failed"/>\n\t\t<system-out>${xml(n.fail)}</system-out>\n\t</testcase>`
        : `\t<testcase name="${xml(n.name)}" classname="${xml(n.name)}" time="0.003" status="run">\n\t\t<system-out></system-out>\n\t</testcase>`,
    )
    .join("\n")}\n</testsuite>\n`;
}

function selected(cfg: RunConfiguration, cases: Case[]): Case[] {
  const o = cfg.options ?? {};
  const scope = o.scope || "all";
  if (scope === "test") {
    const test = o.test ?? "";
    const section = o.section ?? "";
    return cases.filter((c) =>
      cfg.type === "gtest"
        ? (c.suite === o.suite || c.suite.endsWith(`/${o.suite}`)) && (c.name === test || c.name.startsWith(`${test}/`))
        : c.name === test || c.name.startsWith(`${test}/${section}`),
    );
  }
  if (scope === "pattern") {
    const p = (o.pattern || o.filter || "").replace(/\*/g, "");
    return cases.filter((c) => `${c.suite}.${c.name}`.includes(p) || p.split(/[,:]/).some((x) => x && c.name.includes(x.replace(/^\./, ""))));
  }
  return cases;
}

const LCOV = `TN:\nSF:${CPP_ROOT}/src/geometry/polygon.cpp\nFNF:5\nFNH:4\nDA:9,8\nDA:10,8\nDA:11,1\nDA:13,7\nDA:15,3\nDA:16,3\nDA:17,12\nDA:18,12\nDA:19,12\nDA:20,12\nDA:22,3\nDA:25,2\nDA:26,2\nDA:27,6\nDA:28,6\nDA:29,6\nDA:31,2\nDA:34,4\nDA:35,4\nDA:36,4\nDA:37,16\nDA:38,16\nDA:40,4\nDA:43,0\nDA:44,0\nDA:47,5\nDA:48,5\nLF:27\nLH:25\nend_of_record\nTN:\nSF:${CPP_ROOT}/src/stats.cpp\nDA:8,2\nDA:9,2\nDA:10,1\nDA:13,3\nDA:14,3\nDA:15,3\nDA:16,3\nDA:17,3\nLF:8\nLH:8\nend_of_record\n`;

interface CppRun {
  cmd: CommandLine;
  steps: Step[];
  report?: string;
  defaultFile?: string;
  test: boolean;
}

async function cppRun(r: RunRequest): Promise<CppRun | null> {
  if (!isCppDemo()) return null;
  if (r.command && r.command.args[0] === "--build") return { cmd: r.command, steps: buildSteps(r.command.args), test: false };
  const cfg = r.configuration;
  if (!cfg) return null;
  if (cfg.type === "cmake-app") {
    const ctx = await cppContext();
    const exe = cfg.options?.executable || ctx.targets.find((t) => t.name === cfg.options?.target)?.executable || `${ctx.buildDir}/shapes_app`;
    const args = (cfg.arguments ?? "").split(/\s+/).filter(Boolean);
    const stress = args.includes("--stress");
    return {
      cmd: { program: exe, args, cwd: ctx.buildDir, env: cfg.env ?? {} },
      steps: [
        [200, "4 corners, area 4.00, perimeter 8.00\n"],
        [30, "3 corners, area 6.00, perimeter 12.00\n"],
        [30, "4 corners, area 9.00, perimeter 12.00\n"],
        [30, "mean area: 6.33333\n"],
        ...(stress ? ([[60, asanOutput(ctx.buildDir), "stderr"], [30, { code: 1 }]] as Step[]) : ([[30, "median area: 6\n"], [30, { code: 0 }]] as Step[])),
      ],
      test: false,
    };
  }
  if (!["gtest", "catch2", "ctest"].includes(cfg.type)) return null;
  const wasm = await forgeWasm();
  const spec = mockTestFrameworkSpecs().find((s) => s.id === cfg.type);
  const built = callWasmSync<{ program: string; args: string[]; env?: Record<string, string>; defaultFile?: string; error?: string }>(wasm.testing_command, {
    configuration: cfg,
    spec,
    reportDir: "/tmp/forge-test-reports/run",
    coverageFile: r.coverage ? "/tmp/forge-test-reports/run-coverage/coverage" : null,
    cppContext: await cppContext(),
  });
  if (built.error) throw new Error(built.error);
  const cmd: CommandLine = { program: built.program, args: built.args, cwd: CPP_ROOT, env: { ...(built.env ?? {}), ...(cfg.env ?? {}) } };
  const steps: Step[] = [[200, ""]];
  let report = "";
  let failed = false;
  if (cfg.type === "gtest") {
    const cases = selected(cfg, GTEST);
    steps.push([20, `[==========] Running ${cases.length} tests from ${new Set(cases.map((c) => c.suite)).size} test suites.\n`]);
    for (const c of cases) steps.push([40, `[ RUN      ] ${c.suite}.${c.name}\n`], [10, `[       OK ] ${c.suite}.${c.name} (0 ms)\n`]);
    steps.push([20, `[  PASSED  ] ${cases.length} tests.\n`]);
    report = gtestReport(cases);
  } else if (cfg.type === "catch2") {
    const cases = selected(cfg, CATCH);
    failed = cases.some((c) => c.fail);
    for (const c of cases.filter((x) => x.fail)) steps.push([60, `${CPP_ROOT}/tests/stats_test.cpp:${c.line + 1}: FAILED:\n${c.fail!.split("\n").slice(1, 4).join("\n")}\n`, "stdout"]);
    steps.push([20, `===============================================================================\ntest cases: ${cases.length} | ${cases.length - Number(failed)} passed | ${Number(failed)} failed\n`]);
    report = catchReport(cases);
  } else {
    const names: { name: string; fail?: string }[] = [
      ...GTEST.map((c) => ({ name: `${c.suite}.${c.name}` })),
      { name: "mean of areas" },
      { name: "median", fail: "even count FAILED: 3.0 == 2.5" },
      { name: "empty input throws" },
    ];
    failed = true;
    steps.push([20, `Test project ${(await cppContext()).buildDir}\n`]);
    names.forEach((n, i) => steps.push([30, `${String(i + 1).padStart(2)}/${names.length} Test #${i + 1}: ${n.name} ${".".repeat(Math.max(3, 40 - n.name.length))}${n.fail ? "***Failed" : "   Passed"}    0.00 sec\n`]));
    report = ctestReport(names);
  }
  steps.push([20, { report }], [20, { code: failed ? 1 : 0 }]);
  return { cmd, steps, report, defaultFile: built.defaultFile, test: true };
}

registerMockRunHandler({
  commandLine: async (r) => (await cppRun(r))?.cmd ?? null,
  start: async (r, runId) => {
    let run = await cppRun(r);
    if (!run) return null;
    const cfg = r.configuration;
    // Run with Valgrind Memcheck / Profile with perf; else sanitizer reports from the output.
    let exitEvent: (() => Promise<RunEvent | null>) | undefined;
    if (r.nativeTool && cfg && ["cmake-app", "gtest", "catch2"].includes(cfg.type)) {
      const native = nativeRun(r, run.cmd, run.steps);
      run = { ...run, cmd: native.cmd, steps: native.steps };
      exitEvent = native.exitEvent;
    } else if (cfg) {
      const text = run.steps.map((step) => (typeof step[1] === "string" ? step[1] : "")).join("");
      const buildDir = (await cppContext()).buildDir;
      exitEvent = () => sanitizerEvent(text, buildDir);
    }
    return startMockRun(runId, run.cmd, run.steps, {
      exitEvent,
      testRunner: !!r.testRunner,
      testReport: run.test ? "jUnitFile" : undefined,
      junitFlavor: run.test ? "generic" : undefined,
      defaultTestFile: run.defaultFile,
      locationRules: [{ pattern: `^${CPP_ROOT}/`, replacement: "" }],
      configuration: run.test ? cfg : undefined,
      coverage: r.coverage && cfg ? { name: cfg.name, text: () => LCOV, rules: [{ pattern: `^${CPP_ROOT}/`, replacement: "" }] } : undefined,
    });
  },
});

// Run configurations of the demo, like a CLion user's project.
if (isCppDemo() && !isPioDemo()) {
  void Promise.resolve().then(async () => {
    const { mockHandler } = await import("@/ipc");
    const save = mockHandler("run_configurations_save");
    if (!save) return;
    await save({
      configurations: {
        selected: "shapes_app",
        configurations: [
          { name: "shapes_app", type: "cmake-app", storeAsProjectFile: true, options: { target: "shapes_app" } },
          { name: "polygon_tests", type: "gtest", storeAsProjectFile: true, options: { scope: "file", file: "tests/polygon_test.cpp", suites: "PolygonTest|PolygonErrors|ScaleTest" } },
          { name: "stats_tests", type: "catch2", options: { scope: "file", file: "tests/stats_test.cpp", suites: "mean of areas|median|empty input throws" } },
          { name: "All CTest", type: "ctest", storeAsProjectFile: true, options: { scope: "all", jobs: "4" } },
          { name: "shapes_app (stress)", type: "cmake-app", arguments: "--stress", options: { target: "shapes_app" } },
        ],
      },
    });
    // A profile with AddressSanitizer and UndefinedBehaviorSanitizer.
    await mockHandler("settings_set")?.({
      key: "cmake.profiles",
      value: [
        { name: "Debug", buildType: "Debug", toolchain: "", generator: "", cmakeOptions: "", buildDirectory: "", buildOptions: "-j 8", environment: "", enabled: "true", sanitizers: "" },
        { name: "Debug-ASan", buildType: "Debug", toolchain: "", generator: "", cmakeOptions: "", buildDirectory: "", buildOptions: "-j 8", environment: "", enabled: "true", sanitizers: "address,undefined" },
      ],
      scope: "project",
    });
  });
}
