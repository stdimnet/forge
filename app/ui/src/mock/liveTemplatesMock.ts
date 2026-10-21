/**
 * Browser-mode live templates: the same Rust crate as the desktop app
 * (`forge-wasm::livetemplates`), so the mock computes nothing itself.
 */
import { registerMock } from "@/ipc";
import { callWasmSync, forgeWasm, type ForgeWasm } from "@/wasm";

const bridge = (fn: (w: ForgeWasm) => (json: string) => string, key: string) =>
  registerMock(`live_templates_${key}`, async (args) => {
    const w = await forgeWasm();
    const input = key === "catalog" ? args.settings : key === "variables" ? { text: args.text } : args.request;
    return callWasmSync(fn(w), input);
  });

bridge((w) => w.live_templates_catalog, "catalog");
bridge((w) => w.live_templates_applicable, "applicable");
bridge((w) => w.live_templates_surround_items, "surround_items");
bridge((w) => w.live_templates_unwrap, "unwrap");
bridge((w) => w.live_templates_variables, "variables");

// `user()` and the clipboard have no real source in the browser demo.
const withUser = (request: { env?: { user?: string } }) => ({ ...request, env: { ...request.env, user: request.env?.user || "developer" } });
registerMock("live_templates_expand", async ({ request }) =>
  callWasmSync((await forgeWasm()).live_templates_expand, withUser(request as { env?: { user?: string } })),
);
registerMock("live_templates_surround", async ({ request }) =>
  callWasmSync((await forgeWasm()).live_templates_surround, withUser(request as { env?: { user?: string } })),
);
registerMock("live_templates_user", () => "developer");
