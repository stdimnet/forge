import { describe, expect, it } from "vitest";
import type { GoProblem, ModCompletionResult, ModFile, ProfileView } from "@/ipc/go";
import type { TestFile } from "@/ipc/testFrameworks";

// The Go demo is chosen by the page URL, before the mocks load.
history.replaceState(null, "", "/?project=go");
const { invoke } = await import("@/ipc");
const { goDemoFiles } = await import("./demoGoProject");
await import("./settingsMock");
await import("./testFrameworksMock");
await import("./goMock");
const { goTestSteps } = await import("./goTestsMock");

describe("GoForge mock (Rust crates through WebAssembly)", () => {
  it("parses go.mod and completes versions", async () => {
    const parsed = await invoke<ModFile>("go_mod_parse", { path: "go.mod", content: goDemoFiles["go.mod"] });
    expect(parsed.kind).toBe("mod");
    if (parsed.kind !== "mod") return;
    expect(parsed.module?.text).toBe("github.com/acme/ledger");
    expect(parsed.requires.map((r) => [r.path.text, r.indirect])).toEqual([
      ["github.com/google/uuid", false],
      ["golang.org/x/text", false],
      ["github.com/stretchr/testify", true],
    ]);
    expect(parsed.problems).toEqual([]);
    const content = "module a\n\nrequire github.com/google/uuid v1.";
    const done = await invoke<ModCompletionResult>("go_mod_complete", { path: "go.mod", content, offset: content.length });
    expect(done.from).toBe(31);
    expect(done.items[0]).toMatchObject({ label: "v1.6.0", detail: "latest" });
  });

  it("discovers Go tests with subtests for the gutter", async () => {
    const found = await invoke<TestFile>("test_discover", { path: "internal/money/money_test.go", text: goDemoFiles["internal/money/money_test.go"] });
    expect(found.framework).toBe("go-test");
    const keys = found.targets.map((t) => t.key);
    expect(keys).toContain("internal/money::TestRound/negative");
    expect(keys).toContain("internal/money::FuzzParse");
    expect(found.targets.find((t) => t.key === "internal/money::BenchmarkString")?.configuration.options?.kind).toBe("bench");
  });

  it("answers lint, generate and profiles", async () => {
    const lint = await invoke<GoProblem[]>("go_lint", { tool: "golangci", path: "internal/store/store.go" });
    expect(lint.map((p) => [p.path, p.line, p.code])).toEqual([
      ["internal/store/store.go", 23, "errcheck"],
      ["internal/store/store.go", 44, "errcheck"],
    ]);
    const directives = await invoke<{ line: number; command: string }[]>("go_generate_directives", { content: goDemoFiles["internal/report/report.go"] });
    expect(directives).toEqual([{ line: 11, command: "stringer -type=Kind -output=kind_string.go" }]);
    const profile = await invoke<ProfileView>("pprof_open", { path: "/tmp/cpu.pprof" });
    expect(profile.sampleTypes[profile.sampleIndex].kind).toBe("cpu");
    expect(profile.tree.children[0].name).toBe("runtime.goexit");
  });

  it("plays go test -json output with a failing subtest", () => {
    const steps = goTestSteps({ name: "TestRound", type: "go-test", options: { scope: "test", file: "internal/money/money_test.go", test: "TestRound" } });
    const lines = steps.map((s) => s[1]).filter((t): t is string => typeof t === "string" && t.startsWith("{"));
    const events = lines.map((l) => JSON.parse(l) as { Action: string; Test?: string });
    expect(events.filter((e) => e.Action === "fail").map((e) => e.Test)).toEqual(["TestRound/negative", "TestRound", undefined]);
    expect(steps.at(-1)?.[1]).toEqual({ code: 1 });
  });
});
