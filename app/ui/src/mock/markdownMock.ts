/** Browser-mode Markdown: the Rust `forge-markdown` crate via WebAssembly, like the desktop app. */
import { registerMock } from "@/ipc";
import type { HoverDoc, MdBlock, MdDocument } from "@/ipc/markdown";
import { callWasm, forgeWasm } from "@/wasm";

registerMock("markdown_parse", async ({ text, flavor }) => {
  const wasm = await forgeWasm();
  return callWasm<MdBlock[]>(wasm.markdown_parse, { text: String(text ?? ""), flavor: flavor ?? "chat" });
});

registerMock("markdown_hover", async ({ markdown }) => {
  const wasm = await forgeWasm();
  return callWasm<HoverDoc>(wasm.markdown_hover, { markdown: String(markdown ?? "") });
});

registerMock("markdown_document", async ({ text }) => {
  const wasm = await forgeWasm();
  return callWasm<MdDocument>(wasm.markdown_document, { text: String(text ?? "") });
});

registerMock("markdown_anchor_line", async ({ text, anchor }) => {
  const wasm = await forgeWasm();
  return callWasm<number | null>(wasm.markdown_anchor_line, { text: String(text ?? ""), anchor: String(anchor ?? "") });
});
