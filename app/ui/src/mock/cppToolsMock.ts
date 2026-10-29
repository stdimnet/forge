/**
 * Browser-mode CForge v2 for the demo projects: sanitizer reports, Valgrind Memcheck and perf
 * runs of `shapes`, and the vcpkg / Conan commands. Reports are parsed by the same Rust code
 * as on the desktop (WebAssembly: `cpp_sanitizer_reports`, `cpp_valgrind_report`,
 * `cpp_perf_view`, `cpp_vcpkg_complete`); the outputs, Memcheck's XML and perf's samples are
 * canned. Nothing is run.
 */
import { registerMock } from "@/ipc";
import type { NativeToolsInfo, PackageManagers } from "@/ipc/cppTools";
import type { CommandLine, RunEvent, RunRequest } from "@/ipc/run";
import { callWasmSync, forgeWasm } from "@/wasm";
import { cppDemoName, isCppDemo, isPioDemo } from "./demoCppProject";
import { mockFiles } from "./workspaceMock";
import type { Step } from "./runMock";

const ROOT = `/home/anna/Projects/${cppDemoName}`;
const LIBC = "/usr/lib/x86_64-linux-gnu/libc.so.6";

// ---- Sanitizers ---------------------------------------------------------------------------------

/** `shapes_app --stress` in the ASan profile: a heap-buffer-overflow in `shapes::median`. */
export function asanOutput(buildDir: string): string {
  const exe = `${buildDir}/shapes_app`;
  return `=================================================================
==48213==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x503000000058 at pc 0x55c1e4a0b6f2 bp 0x7ffd5e0a1c30 sp 0x7ffd5e0a1c20
READ of size 8 at 0x503000000058 thread T0
    #0 0x55c1e4a0b6f1 in shapes::median(std::vector<double, std::allocator<double> >) ${ROOT}/src/stats.cpp:18
    #1 0x55c1e4a08d3e in main ${ROOT}/src/main.cpp:20
    #2 0x7f2b8e62a1c9 in __libc_start_call_main ../sysdeps/nptl/libc_start_call_main.h:58
    #3 0x7f2b8e62a28a in __libc_start_main_impl ../csu/libc-start.c:360
    #4 0x55c1e4a08a24 in _start (${exe}+0x7a24) (BuildId: 2f0c8d1a77e5b1c3)

0x503000000058 is located 0 bytes after 24-byte region [0x503000000040,0x503000000058)
allocated by thread T0 here:
    #0 0x7f2b8eefe548 in operator new(unsigned long) ../../../../src/libsanitizer/asan/asan_new_delete.cpp:95
    #1 0x55c1e4a0a1b9 in std::__new_allocator<double>::allocate(unsigned long, void const*) /usr/include/c++/13/bits/new_allocator.h:151
    #2 0x55c1e4a09e5d in std::vector<double, std::allocator<double> >::vector(std::vector<double, std::allocator<double> > const&) /usr/include/c++/13/bits/stl_vector.h:603
    #3 0x55c1e4a08d1f in main ${ROOT}/src/main.cpp:20
    #4 0x7f2b8e62a1c9 in __libc_start_call_main ../sysdeps/nptl/libc_start_call_main.h:58

SUMMARY: AddressSanitizer: heap-buffer-overflow ${ROOT}/src/stats.cpp:18 in shapes::median(std::vector<double, std::allocator<double> >)
Shadow bytes around the buggy address:
  0x502ffffffd80: 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
=>0x503000000000: fa fa 00 00 00 fa fa fa 00 00 00[fa]fa fa fa fa
==48213==ABORTING
`;
}

/** The sanitizer event of a finished demo run (what the backend's observer emits). */
export async function sanitizerEvent(output: string, buildDir: string): Promise<RunEvent | null> {
  if (!output.includes("Sanitizer") && !output.includes("runtime error:")) return null;
  const wasm = await forgeWasm();
  const reports = callWasmSync<unknown[]>(wasm.cpp_sanitizer_reports, {
    text: output,
    root: ROOT,
    buildDir: buildDir.startsWith(`${ROOT}/`) ? buildDir.slice(ROOT.length + 1) : "",
    files: Object.keys(mockFiles),
  });
  return reports.length ? ({ type: "sanitizer", reports } as RunEvent) : null;
}

// ---- Valgrind Memcheck --------------------------------------------------------------------------

const frame = (fn: string, file?: string, line?: number, obj = `${ROOT}/cmake-build-debug/shapes_app`) =>
  `    <frame>
      <ip>0x10${(fn.length * 4099).toString(16).toUpperCase()}</ip>
      <obj>${obj}</obj>
      <fn>${fn.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")}</fn>${
        file
          ? `
      <dir>${ROOT}/${file.split("/").slice(0, -1).join("/")}</dir>
      <file>${file.split("/").pop()}</file>
      <line>${line}</line>`
          : ""
      }
    </frame>`;

const PRELOAD = "/usr/libexec/valgrind/vgpreload_memcheck-amd64-linux.so";

function memcheckXml(exe: string): string {
  return `<?xml version="1.0"?>
<valgrindoutput>
<protocolversion>4</protocolversion>
<protocoltool>memcheck</protocoltool>
<pid>51234</pid>
<tool>memcheck</tool>
<args>
  <argv>
    <exe>${exe}</exe>
  </argv>
</args>
<status><state>RUNNING</state></status>
<error>
  <unique>0x0</unique>
  <tid>1</tid>
  <kind>InvalidRead</kind>
  <what>Invalid read of size 8</what>
  <stack>
${frame("shapes::median(std::vector<double, std::allocator<double> >)", "src/stats.cpp", 18)}
${frame("main", "src/main.cpp", 20)}
  </stack>
  <auxwhat>Address 0x4e2a0d8 is 0 bytes after a block of size 24 alloc'd</auxwhat>
  <stack>
${frame("operator new(unsigned long)", undefined, undefined, PRELOAD)}
${frame("std::vector<double, std::allocator<double> >::vector(std::vector<double, std::allocator<double> > const&)", undefined, undefined)}
${frame("main", "src/main.cpp", 20)}
  </stack>
</error>
<error>
  <unique>0x1</unique>
  <tid>1</tid>
  <kind>UninitCondition</kind>
  <what>Conditional jump or move depends on uninitialised value(s)</what>
  <stack>
${frame("shapes::mean(std::vector<double, std::allocator<double> > const&)", "src/stats.cpp", 10)}
${frame("main", "src/main.cpp", 19)}
  </stack>
  <auxwhat>Uninitialised value was created by a stack allocation</auxwhat>
  <stack>
${frame("main", "src/main.cpp", 7)}
  </stack>
</error>
<error>
  <unique>0x2</unique>
  <tid>1</tid>
  <kind>InvalidWrite</kind>
  <what>Invalid write of size 8</what>
  <stack>
${frame("shapes::Polygon::scaled(double) const", "src/geometry/polygon.cpp", 38)}
${frame("main", "src/main.cpp", 11)}
  </stack>
  <auxwhat>Address 0x4e2b140 is 0 bytes after a block of size 64 alloc'd</auxwhat>
  <stack>
${frame("operator new(unsigned long)", undefined, undefined, PRELOAD)}
${frame("shapes::Polygon::scaled(double) const", "src/geometry/polygon.cpp", 36)}
${frame("main", "src/main.cpp", 11)}
  </stack>
</error>
<status><state>FINISHED</state></status>
<error>
  <unique>0x5</unique>
  <tid>1</tid>
  <kind>Leak_DefinitelyLost</kind>
  <xwhat>
    <text>72 (24 direct, 48 indirect) bytes in 1 blocks are definitely lost in loss record 2 of 3</text>
    <leakedbytes>72</leakedbytes>
    <leakedblocks>1</leakedblocks>
  </xwhat>
  <stack>
${frame("operator new(unsigned long)", undefined, undefined, PRELOAD)}
${frame("shapes::square(double)", "src/geometry/polygon.cpp", 48)}
${frame("main", "src/main.cpp", 9)}
  </stack>
</error>
<error>
  <unique>0x6</unique>
  <tid>1</tid>
  <kind>Leak_PossiblyLost</kind>
  <xwhat>
    <text>16 bytes in 1 blocks are possibly lost in loss record 1 of 3</text>
    <leakedbytes>16</leakedbytes>
    <leakedblocks>1</leakedblocks>
  </xwhat>
  <stack>
${frame("malloc", undefined, undefined, PRELOAD)}
${frame("shapes::Polygon::describe() const", "src/geometry/polygon.cpp", 44)}
${frame("main", "src/main.cpp", 16)}
  </stack>
</error>
<errorcounts>
  <pair><count>3</count><unique>0x0</unique></pair>
  <pair><count>1</count><unique>0x1</unique></pair>
  <pair><count>1</count><unique>0x2</unique></pair>
</errorcounts>
</valgrindoutput>
`;
}

// ---- perf ---------------------------------------------------------------------------------------

/** Call stacks of `shapes_app` (caller first) and how many samples each got. */
const PERF_STACKS: [number, [string, string?, number?][]][] = [
  [118, [["main", "src/main.cpp", 16], ["shapes::Polygon::describe() const", "src/geometry/polygon.cpp", 44], ["std::vformat[abi:cxx11](std::basic_string_view<char, std::char_traits<char> >, std::format_args)"]]],
  [64, [["main", "src/main.cpp", 16], ["shapes::Polygon::describe() const", "src/geometry/polygon.cpp", 44], ["shapes::Polygon::perimeter() const", "src/geometry/polygon.cpp", 29], ["__hypot_finite"]]],
  [41, [["main", "src/main.cpp", 16], ["shapes::Polygon::describe() const", "src/geometry/polygon.cpp", 44], ["shapes::Polygon::area() const", "src/geometry/polygon.cpp", 19]]],
  [57, [["main", "src/main.cpp", 17], ["shapes::Polygon::area() const", "src/geometry/polygon.cpp", 19]]],
  [23, [["main", "src/main.cpp", 17], ["shapes::Polygon::area() const", "src/geometry/polygon.cpp", 21], ["std::abs(double)"]]],
  [48, [["main", "src/main.cpp", 20], ["shapes::median(std::vector<double, std::allocator<double> >)", "src/stats.cpp", 16], ["void std::__introsort_loop<__gnu_cxx::__normal_iterator<double*, std::vector<double, std::allocator<double> > >, long, __gnu_cxx::__ops::_Iter_less_iter>(…)"]]],
  [19, [["main", "src/main.cpp", 19], ["shapes::mean(std::vector<double, std::allocator<double> > const&)", "src/stats.cpp", 11]]],
  [31, [["main", "src/main.cpp", 11], ["shapes::Polygon::scaled(double) const", "src/geometry/polygon.cpp", 38], ["std::vector<shapes::Point, std::allocator<shapes::Point> >::push_back(shapes::Point&&)"]]],
  [27, [["main", "src/main.cpp", 16], ["std::ostream::operator<<(…)"], ["__vfprintf_internal"]]],
  [12, [["main", "src/main.cpp", 9], ["shapes::square(double)", "src/geometry/polygon.cpp", 48], ["malloc"]]],
];

/** `perf script` output of the demo profile (leaf first, with source lines). */
function perfScript(exe: string): string {
  const out: string[] = [];
  let t = 91234.5;
  let addr = 0x55c1e4a08000;
  for (const [count, stack] of PERF_STACKS) {
    for (let i = 0; i < count; i++) {
      t += 0.001001;
      out.push(`shapes_app 52011/52011 ${t.toFixed(6)}:    1001001 cpu-clock:ppp: `);
      for (const [fn, file, line] of [...stack].reverse()) {
        addr += 0x10;
        const dso = file ? exe : /^(__|malloc|std::vformat|std::ostream)/.test(fn) ? LIBC : exe;
        out.push(`\t    ${addr.toString(16)} ${fn}+0x${(fn.length * 3).toString(16)} (${dso})`);
        if (file) out.push(`  ${ROOT}/${file}:${line}`);
      }
      out.push(`\t    7f2b8e62a1c9 __libc_start_call_main+0x79 (${LIBC})`);
      out.push(`\t    7f2b8e62a28a __libc_start_main@@GLIBC_2.34+0x8a (${LIBC})`);
      out.push(`\t    55c1e4a08a24 _start+0x24 (${exe})`);
      out.push("");
    }
  }
  return out.join("\n");
}

// ---- Wrapping runs --------------------------------------------------------------------------------

/** Run with Valgrind Memcheck / Profile with perf in browser mode: the wrapped command, its
 * extra output and the event the backend emits at the exit. */
export function nativeRun(r: RunRequest, cmd: CommandLine, steps: Step[]): { cmd: CommandLine; steps: Step[]; exitEvent: () => Promise<RunEvent | null> } {
  const exe = cmd.program;
  if (r.nativeTool === "valgrind") {
    const xmlFile = "/tmp/forge-valgrind-run.xml";
    return {
      cmd: { ...cmd, program: "/usr/bin/valgrind", args: ["--tool=memcheck", "--xml=yes", `--xml-file=${xmlFile}`, "--leak-check=full", "--track-origins=yes", exe, ...cmd.args] },
      steps: [[300, "==51234== Memcheck, a memory error detector\n==51234== Using Valgrind-3.22.0 and LibVEX; rerun with -h for copyright info\n", "stderr"], ...steps.map((s): Step => [s[0] * 4, ...s.slice(1)] as Step)],
      exitEvent: async () => {
        const wasm = await forgeWasm();
        const report = callWasmSync(wasm.cpp_valgrind_report, { text: memcheckXml(exe), root: ROOT, files: Object.keys(mockFiles) });
        return { type: "valgrind", report } as RunEvent;
      },
    };
  }
  const data = "/tmp/forge-perf-run.data";
  return {
    cmd: { ...cmd, program: "/usr/bin/perf", args: ["record", "-o", data, "--call-graph", "dwarf", "-F", "999", "--", exe, ...cmd.args] },
    steps: [
      ...steps,
      [200, `[ perf record: Woken up 3 times to write data ]\n[ perf record: Captured and wrote 4.817 MB ${data} (440 samples) ]\n`, "stderr"],
    ],
    exitEvent: async () => {
      const wasm = await forgeWasm();
      const view = callWasmSync(wasm.cpp_perf_view, { text: perfScript(exe) });
      return { type: "perf", profile: { title: exe.split("/").pop() ?? exe, view } } as RunEvent;
    },
  };
}

// ---- Commands ---------------------------------------------------------------------------------

registerMock("cpp_sanitizer_reports", async ({ output }) => {
  const e = await sanitizerEvent(String(output), `${ROOT}/cmake-build-debug`);
  return e?.type === "sanitizer" ? e.reports : [];
});

registerMock("cpp_native_tools", (): NativeToolsInfo => ({ valgrind: "/usr/bin/valgrind", perf: "/usr/bin/perf", remote: false }));

registerMock("cpp_valgrind_open", async () => {
  const wasm = await forgeWasm();
  return callWasmSync(wasm.cpp_valgrind_report, { text: memcheckXml(`${ROOT}/cmake-build-debug/shapes_app`), root: ROOT, files: Object.keys(mockFiles) });
});

const VCPKG_ROOT = "/home/anna/vcpkg";
const PORTS = ["abseil", "benchmark", "boost-asio", "catch2", "cli11", "eigen3", "fmt", "glm", "gtest", "nlohmann-json", "openssl", "range-v3", "spdlog", "sqlite3", "tl-expected", "zlib"];

registerMock("cpp_packages", (): PackageManagers => {
  if (!isCppDemo() || isPioDemo()) return {};
  const toolchainFile = `${VCPKG_ROOT}/scripts/buildsystems/vcpkg.cmake`;
  return { vcpkg: { root: VCPKG_ROOT, toolchainFile, dependencies: ["fmt", "gtest", "catch2"] }, toolchainFile };
});

registerMock("cpp_vcpkg_complete", async ({ text, offset }) => callWasmSync((await forgeWasm()).cpp_vcpkg_complete, { text, offset, ports: PORTS }));

registerMock("cpp_conan_install_command", (): CommandLine => ({
  program: "/home/anna/.local/bin/conan",
  args: ["install", ".", "-of", `${ROOT}/cmake-build-debug`, "-s", "build_type=Debug", "--build=missing"],
  cwd: ROOT,
  env: {},
}));

registerMock("cpp_clangd_restart", () => undefined);
