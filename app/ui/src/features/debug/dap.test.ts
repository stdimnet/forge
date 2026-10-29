import { describe, expect, it } from "vitest";
import type { Property } from "./api";
import { formatValue, frameLabel, genericScopeStart, inlineValuesGeneric } from "./format";
import { wantsCompletion } from "./ExpressionInput";
import { defaultLaunchRequest } from "./adapters";

const dap = (p: Partial<Property>): Property => ({
  name: "x",
  fullname: "x",
  type: "dap",
  hasChildren: false,
  numChildren: 0,
  page: 0,
  pageSize: 0,
  children: [],
  ...p,
});

const DEMO = `int compute(int n) {
    int sum = 0;
    return sum;
}

int main(void) {
    int counter = 0;
    char name[8] = "forge";
    counter = compute(4); // counter is set here
    printf("%s %d\\n", name, counter);
}`.split("\n");

describe("debug adapter values", () => {
  it("shows the language's type and value", () => {
    expect(formatValue(dap({ classname: "int", value: "6" }))).toBe("{int} 6");
    expect(formatValue(dap({ classname: "struct point", value: "{x=3, y=4}", hasChildren: true, reference: 7 }))).toBe("{struct point} {x=3, y=4}");
    expect(formatValue(dap({ value: "0x7ffe" }))).toBe("0x7ffe");
    expect(formatValue(dap({ type: "scope", name: "Globals" }))).toBe("");
  });

  it("labels native frames like CLion", () => {
    expect(frameLabel({ level: 0, file: "src/demo.c", fileUri: "/w/src/demo.c", line: 17, where: "main", id: 1 })).toEqual({ location: "demo.c:17", where: "main" });
    expect(frameLabel({ level: 1, file: null, fileUri: "", line: 0, where: "__futex_wait", id: 51, address: "0x00007ffff7c91117" })).toEqual({
      location: "0x00007ffff7c91117",
      where: "__futex_wait",
    });
  });

  it("finds function starts and inline values in C", () => {
    expect(genericScopeStart(DEMO, 9)).toBe(6);
    expect(genericScopeStart(DEMO, 3)).toBe(1);
    const values = inlineValuesGeneric(DEMO, 9, [
      dap({ name: "counter", classname: "int", value: "6" }),
      dap({ name: "name", classname: "char[8]", value: '"forge"' }),
      dap({ type: "scope", name: "Globals" }),
    ]);
    expect(values.get(7)).toBe("counter: 6");
    expect(values.get(8)).toBe('name: "forge"');
    // Comments and calls of unknown names are skipped.
    expect(values.get(9)).toBe("counter: 6");
    expect(values.has(10)).toBe(false);
  });

  it("asks for completions on identifiers and members", () => {
    expect(wantsCompletion("coun", true)).toBe(true);
    expect(wantsCompletion("origin.", true)).toBe(true);
    expect(wantsCompletion("1 + ", true)).toBe(false);
    expect(wantsCompletion("coun", false)).toBe(false);
    expect(wantsCompletion("$ord", false)).toBe(true);
  });

  it("builds launch requests from run configurations", () => {
    const r = defaultLaunchRequest("lldb", {
      name: "demo",
      type: "native-app",
      arguments: "--fast 'a b'",
      workingDirectory: "build",
      options: { program: "build/demo", stopOnEntry: "true", launchOptions: '{"initCommands": ["x"]}' },
    });
    expect(r).toMatchObject({ mode: "launch", adapter: "lldb", program: "build/demo", arguments: "--fast 'a b'", cwd: "build", stopOnEntry: true });
    expect(r.configuration).toEqual({ initCommands: ["x"] });
    expect(() => defaultLaunchRequest("lldb", { name: "x", type: "native-app", options: { launchOptions: "{" } })).toThrow(/JSON/);
  });
});
