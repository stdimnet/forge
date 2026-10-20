/** Browser-mode Markdown: the Rust `forge-markdown` crate via WebAssembly, like the desktop app. */
import { registerMock } from "@/ipc";
import type { HoverDoc, MdBlock } from "@/ipc/markdown";
import { callWasm, forgeWasm } from "@/wasm";

registerMock("markdown_parse", async ({ text, flavor }) => {
  const wasm = await forgeWasm();
  return callWasm<MdBlock[]>(wasm.markdown_parse, { text: String(text ?? ""), flavor: flavor ?? "chat" });
});

registerMock("markdown_hover", async ({ markdown }) => {
  const wasm = await forgeWasm();
  return callWasm<HoverDoc>(wasm.markdown_hover, { markdown: String(markdown ?? "") });
});
