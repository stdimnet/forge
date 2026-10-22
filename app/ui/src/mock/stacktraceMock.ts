/**
 * Browser-mode Analyze Stack Trace: the same Rust analyzer (`forge_output::stacktrace`,
 * WebAssembly) over the demo project's files, with the demo Docker setup's mapping of
 * `/var/www/html` to the project root.
 */
import { registerMock } from "@/ipc";
import type { TraceAnalysis } from "@/ipc/stacktrace";
import { callWasm, forgeWasm } from "@/wasm";
import { mockFiles } from "./workspaceMock";

registerMock("stacktrace_analyze", async ({ text }) => {
  const w = await forgeWasm();
  return callWasm<TraceAnalysis>(w.stacktrace_analyze, {
    text,
    context: { root: "/home/dev/Projects/acme-shop", mappings: [{ local: "", remote: "/var/www/html" }] },
    files: Object.keys(mockFiles),
  });
});
