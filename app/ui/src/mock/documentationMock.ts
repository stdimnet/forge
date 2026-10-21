/**
 * Browser-mode documentation actions of the View menu (the desktop app answers from
 * app/src-tauri/src/commands/codeintel.rs): External Documentation, Quick Definition, Type
 * Info and Context Info. Declarations come from the Go to Declaration mock, hovers from the
 * language server mock; the URLs, snippets, headers and types are computed by the Rust code
 * (`forge_codeintel::docs`) as WebAssembly.
 */
import { invoke, registerMock } from "@/ipc";
import type {
  ContextLine,
  DeclarationAnswer,
  ExternalDocAnswer,
  LibraryFile,
  NavTarget,
  QuickDefinition,
  QuickDefinitionAnswer,
  SymbolHover,
} from "@/ipc/codeintel";
import type { EditorHover } from "@/ipc/lsp";
import { callWasm, forgeWasm } from "@/wasm";
import { mockFiles } from "./workspaceMock";

type At = {
  path: string;
  line: number;
  character: number;
  content?: string;
};

const isPhp = (p: string) => /\.(php|phtml|inc)$/i.test(p) && !p.endsWith(".blade.php");

async function textOf(path: string, content?: string): Promise<string | undefined> {
  if (typeof content === "string") return content;
  if (path.startsWith("library://")) {
    return invoke<LibraryFile>("library_read", { path })
      .then((f) => f.text)
      .catch(() => undefined);
  }
  return mockFiles[path];
}

const declaration = (a: At) =>
  invoke<DeclarationAnswer>("code_goto_declaration", a).catch(
    (): DeclarationAnswer => ({ onDeclaration: false, targets: [], source: "none" }),
  );

registerMock("code_external_doc", async (args): Promise<ExternalDocAnswer> => {
  const a = args as unknown as At;
  const text = (await textOf(a.path, a.content)) ?? "";
  const target = isPhp(a.path) ? (await declaration(a)).targets[0] : undefined;
  const w = await forgeWasm();
  const out = await callWasm<{ url: string | null; symbol: { word: string } | null }>(w.codeintel_doc_url, {
    text,
    path: a.path,
    position: { line: a.line, character: a.character },
    declaration: target ? { path: target.path, name: target.name, container: target.container ?? null } : null,
  });
  return { url: out.url, name: out.symbol?.word ?? null };
});

registerMock("code_quick_definition", async (args): Promise<QuickDefinitionAnswer> => {
  const a = args as unknown as At;
  const answer = await declaration(a);
  const targets: NavTarget[] = answer.onDeclaration
    ? [{ path: a.path, line: a.line, column: a.character, name: answer.title ?? "" }]
    : answer.targets;
  const w = await forgeWasm();
  const definitions: QuickDefinition[] = [];
  for (const t of targets) {
    const text = t.path === a.path ? await textOf(a.path, a.content) : await textOf(t.path);
    if (text === undefined) continue;
    const range = await callWasm<{ startLine: number; endLine: number; truncated: boolean }>(w.codeintel_definition_snippet, {
      text,
      line: t.line,
      path: t.path,
    });
    definitions.push({
      ...t,
      container: t.container ?? null,
      startLine: range.startLine,
      text: text
        .split("\n")
        .slice(range.startLine, range.endLine + 1)
        .join("\n"),
      truncated: range.truncated,
    });
  }
  return { title: answer.title ?? null, definitions, message: definitions.length ? null : "No definition found" };
});

registerMock("code_type_info", async (args): Promise<string | null> => {
  const a = args as unknown as At;
  const hover = await invoke<EditorHover | null>("lsp_hover", a).catch(() => null);
  const signature = isPhp(a.path)
    ? ((await invoke<SymbolHover | null>("code_symbol_info", a).catch(() => null))?.signature ?? null)
    : null;
  const w = await forgeWasm();
  return callWasm<string | null>(w.codeintel_type_info, { hover: hover?.contents ?? null, signature });
});

registerMock("code_context_info", async (args): Promise<ContextLine[]> => {
  const { path, line, content } = args as { path: string; line: number; content?: string };
  const text = (await textOf(path, content)) ?? "";
  const w = await forgeWasm();
  return callWasm<ContextLine[]>(w.codeintel_context_headers, { text, line, path });
});
