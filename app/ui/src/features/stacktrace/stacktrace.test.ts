import { describe, expect, it } from "vitest";
import { callWasm, forgeWasm } from "@/wasm";
import type { TraceAnalysis } from "@/ipc/stacktrace";

describe("Analyze Stack Trace through Rust", () => {
  it("links frames of a container trace to project files", async () => {
    const w = await forgeWasm();
    const a = await callWasm<TraceAnalysis>(w.stacktrace_analyze, {
      text: "Error: boom\n#0 /var/www/html/app/Models/Order.php(12): x()\n#1 /var/www/html/vendor/a/B.php(3): y()",
      context: { mappings: [{ local: "", remote: "/var/www/html" }] },
      files: ["app/Models/Order.php"],
    });
    expect(a.kind).toBe("php");
    expect([a.frames, a.resolved]).toEqual([2, 1]);
    const links = a.lines.flatMap((l) => l.segments).filter((s) => s.link);
    expect(links.map((s) => [s.link!.target, s.link!.line])).toEqual([["app/Models/Order.php", 12]]);
  });
});
