import { describe, expect, it } from "vitest";
import { forgeWasm } from "./index";

describe("forge-wasm bridge", () => {
  it("loads the WebAssembly module", async () => {
    const wasm = await forgeWasm();
    expect(wasm.bridge_version()).toMatch(/^\d+\.\d+\.\d+/);
  });
});
