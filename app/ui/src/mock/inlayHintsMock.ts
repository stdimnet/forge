/**
 * Browser-mode inlay hints: a small "server" that reports what Intelephense would on the demo
 * project (parameter names at literal arguments, variable and closure return types). Grouping
 * and filtering by the settings run through the same Rust code as the desktop app
 * (`forge_codeintel::inlay` as WebAssembly).
 */
import { registerMock } from "@/ipc";
import type { RawInlayHint } from "@/ipc/lsp";
import { callWasm, forgeWasm } from "@/wasm";
import { signatures } from "./lspDemoData";
import { mockSettingValues } from "./settingsMock";

/** Parameter names of calls the demo data has no signature for. */
const PARAMS: Record<string, string[]> = {
  round: ["num", "precision"],
  authorize: ["ability", "arguments"],
  load: ["relations"],
  with: ["relations"],
  define: ["constant_name", "value"],
  microtime: ["as_float"],
  decimal: ["column", "total", "places"],
  string: ["column"],
};

/** Inferred types of demo variables. */
const TYPES: Record<string, string> = {
  $user: "User|null",
  $lines: "Collection",
  $total: "float",
  $discount: "float",
};

const LITERAL = /^\s*('[^']*'|"[^"]*"|-?\d+(\.\d+)?|true|false|null)\s*$/i;

function paramNames(name: string): string[] | undefined {
  const sig = signatures[name];
  if (sig) return sig.params.map((p) => p.match(/\$(\w+)/)?.[1] ?? p);
  return PARAMS[name];
}

/** Top-level arguments of the call whose `(` is at `open`: [start column, text][]. */
function argumentsAt(line: string, open: number): [number, string][] {
  const out: [number, string][] = [];
  let depth = 0;
  let start = open + 1;
  let quote: string | null = null;
  for (let i = open + 1; i < line.length; i++) {
    const c = line[i];
    if (quote) {
      if (c === quote) quote = null;
      continue;
    }
    if (c === "'" || c === '"') quote = c;
    else if (c === "(" || c === "[") depth++;
    else if ((c === ")" || c === "]") && depth > 0) depth--;
    else if ((c === "," || c === ")") && depth === 0) {
      const text = line.slice(start, i);
      if (text.trim()) out.push([start + (text.length - text.trimStart().length), text]);
      if (c === ")") break;
      start = i + 1;
    }
  }
  return out;
}

export function demoInlayHints(text: string, startLine: number, endLine: number): RawInlayHint[] {
  const hints: RawInlayHint[] = [];
  const lines = text.split("\n");
  for (let n = startLine; n <= Math.min(endLine, lines.length - 1); n++) {
    const line = lines[n];
    for (const m of line.matchAll(/(\w+)\s*\(/g)) {
      const names = paramNames(m[1]);
      if (!names || /function\s*$/.test(line.slice(0, m.index))) continue;
      argumentsAt(line, m.index + m[0].length - 1).forEach(([col, arg], i) => {
        if (names[i] && LITERAL.test(arg)) hints.push({ line: n, character: col, label: `${names[i]}:`, kind: 2, paddingLeft: false, paddingRight: true });
      });
    }
    const assign = line.match(/^(\s*)(\$\w+)\s*=[^=]/);
    if (assign && TYPES[assign[2]]) {
      hints.push({ line: n, character: assign[1].length + assign[2].length, label: `: ${TYPES[assign[2]]}`, kind: 1, paddingLeft: false, paddingRight: false });
    }
    for (const m of line.matchAll(/fn\s*\(\$line\)/g)) {
      hints.push({ line: n, character: m.index + m[0].length, label: ": float", kind: 1, paddingLeft: false, paddingRight: false });
    }
  }
  return hints;
}

registerMock("lsp_inlay_hints", async ({ text, startLine, endLine, languageId }) => {
  if (languageId !== "php") return [];
  const wasm = await forgeWasm();
  return callWasm(wasm.codeintel_inlay_hints, {
    hints: demoInlayHints(String(text), Number(startLine), Number(endLine)),
    text: String(text),
    languageId,
    settings: mockSettingValues(),
  });
});
