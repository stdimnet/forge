/**
 * Browser-mode Extract / Introduce and Inline (the desktop app computes them in Rust on the
 * tree-sitter syntax tree, see app/src-tauri/src/commands/refactor.rs). The tree-sitter
 * grammar is not part of the WebAssembly build, so these mocks approximate the refactorings
 * on the demo project's PHP with text heuristics: enough to try the choosers, dialogs and the
 * in-place name editing. Previews run the Rust code (`forge-codeintel`) as WebAssembly.
 */
import { registerMock } from "@/ipc";
import type { FilePreview, TextEditLsp, WorkspaceEdit } from "@/ipc/codeintel";
import type { LspPosition, LspRange } from "@/ipc/lsp";
import type {
  ExtractMethodInfo,
  ExtractMethodOptions,
  InlineInfo,
  InlineMode,
  IntroduceInfo,
  IntroduceKind,
  IntroduceOptions,
  IntroducePrepareAnswer,
  RefactorAnswer,
} from "@/ipc/refactor";
import { callWasm, forgeWasm } from "@/wasm";
import { offsetOf, positionOf } from "./codeintelMock";
import { mockFiles } from "./workspaceMock";

const CANNOT = "Cannot perform refactoring.";
const NOT_EXPRESSION = `${CANNOT} Selected block should represent an expression`;

const isPhp = (p: string) => /\.(php|phtml|inc)$/i.test(p) && !p.endsWith(".blade.php");

interface Edit {
  start: number;
  end: number;
  text: string;
}

const rangeOf = (text: string, s: number, e: number): LspRange => ({
  start: positionOf(text, s),
  end: positionOf(text, e),
});

function apply(text: string, edits: Edit[]): string {
  let out = text;
  for (const e of [...edits].sort((a, b) => b.start - a.start)) out = out.slice(0, e.start) + e.text + out.slice(e.end);
  return out;
}

/** The new name's ranges after the edits (marked with \u0003 ... \u0004 in edit texts). */
function finish(text: string, edits: Edit[]): { edits: Edit[]; spans: [number, number][] } {
  const sorted = [...edits].sort((a, b) => a.start - b.start);
  const spans: [number, number][] = [];
  let delta = 0;
  const clean = sorted.map((e) => {
    let out = "";
    let open = -1;
    for (const ch of e.text) {
      if (ch === "\u0003") open = e.start + delta + out.length;
      else if (ch === "\u0004") spans.push([open, e.start + delta + out.length]);
      else out += ch;
    }
    delta += out.length - (e.end - e.start);
    return { ...e, text: out };
  });
  void text;
  return { edits: clean, spans };
}

const mark = (name: string) => `\u0003${name}\u0004`;

async function answer(path: string, text: string, raw: Edit[], conflicts: RefactorAnswer["conflicts"] = []): Promise<RefactorAnswer> {
  const { edits, spans } = finish(text, raw);
  const after = apply(text, edits);
  const lsp: TextEditLsp[] = edits.map((e) => ({
    range: rangeOf(text, e.start, e.end),
    newText: e.text,
  }));
  const edit: WorkspaceEdit = {
    changes: lsp.length ? [{ kind: "edit", path, edits: lsp }] : [],
  };
  const wasm = await forgeWasm();
  const preview = await callWasm<FilePreview[]>(wasm.codeintel_preview_edit, {
    edit,
    files: { ...mockFiles, [path]: text },
  });
  return {
    edit,
    preview,
    conflicts,
    nameRanges: spans.map(([s, e]) => rangeOf(after, s, e)),
    skipped: 0,
  };
}

const camel = (words: string[]) => words.map((w, i) => (i ? w.charAt(0).toUpperCase() + w.slice(1) : w)).join("");
const wordsOf = (name: string) =>
  name
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .split(/[^A-Za-z0-9]+/)
    .filter(Boolean)
    .map((w) => w.toLowerCase());

/** Name suggestions like the Rust ones (`$order->lines()` -> `lines`). */
function suggest(expr: string): string[] {
  const e = expr.trim();
  const call = /(?:->|::)\s*(\w+)\s*\([^()]*\)\s*$/.exec(e) ?? /^(\w+)\s*\(/.exec(e);
  const prop = /->\s*(\w+)\s*$/.exec(e);
  const str = /^['"]([\w .-]{1,30})['"]$/.exec(e);
  let base: string[] = [];
  if (call) base = wordsOf(call[1]).filter((w, i, all) => !(i === 0 && all.length > 1 && /^(get|find|load|to)$/.test(w)));
  else if (prop) base = wordsOf(prop[1]);
  else if (str) base = wordsOf(str[1]);
  else if (/^\d+(\.\d+)?$/.test(e)) base = ["number"];
  else if (/^\$\w+$/.test(e)) base = wordsOf(e.slice(1));
  else if (/\*|\+|-|\//.test(e)) base = ["result"];
  if (!base.length) base = ["value"];
  const out: string[] = [];
  for (let i = 0; i < base.length; i++) out.push(camel(base.slice(i)));
  return out;
}

/** The method or function around `offset`: its body range and name. */
function enclosingFunction(
  text: string,
  offset: number,
): {
  name: string;
  start: number;
  open: number;
  close: number;
  indent: string;
} | null {
  const re = /^([ \t]*)(?:(?:public|protected|private|static|final|abstract)\s+)*function\s+(\w+)\s*\(([^)]*)\)[^{;]*\{/gm;
  let found = null;
  for (let m = re.exec(text); m; m = re.exec(text)) {
    const open = m.index + m[0].length - 1;
    let depth = 0;
    let close = open;
    for (let i = open; i < text.length; i++) {
      if (text[i] === "{") depth++;
      else if (text[i] === "}" && --depth === 0) {
        close = i;
        break;
      }
    }
    if (open < offset && offset < close) found = { name: m[2], start: m.index, open, close, indent: m[1] };
  }
  return found;
}

/** Expressions at the caret: the variable / call chain, its call, the assignment's value. */
function candidates(text: string, offset: number): [number, number][] {
  const lineStart = text.lastIndexOf("\n", offset - 1) + 1;
  const lineEnd = text.indexOf("\n", offset);
  const line = text.slice(lineStart, lineEnd < 0 ? text.length : lineEnd);
  const at = offset - lineStart;
  const out: [number, number][] = [];
  const chain = /\$\w+(?:\s*->\s*\w+(?:\([^()]*\))?)*/g;
  for (let m = chain.exec(line); m; m = chain.exec(line)) {
    if (m.index <= at && at <= m.index + m[0].length) {
      // Every prefix of the chain that contains the caret.
      const parts = m[0].split(/(?=->)/);
      let len = 0;
      for (const p of parts) {
        len += p.length;
        if (m.index + len >= at) out.push([lineStart + m.index, lineStart + m.index + len]);
      }
    }
  }
  const value = /(?:=|return)\s+(.+?);\s*(?:\/\/.*)?$/.exec(line);
  if (value) {
    const s = lineStart + line.indexOf(value[1], value.index);
    const r: [number, number] = [s, s + value[1].length];
    if (r[0] <= offset && offset <= r[1] && !out.some((o) => o[0] === r[0] && o[1] === r[1])) out.push(r);
  }
  return out;
}

function trimmed(text: string, s: number, e: number): [number, number] {
  while (s < e && /\s/.test(text[s])) s++;
  while (e > s && /[\s;]/.test(text[e - 1])) e--;
  return [s, e];
}

function occurrencesOf(text: string, expr: string, from: number, to: number): number[] {
  const out: number[] = [];
  for (let i = text.indexOf(expr, from); i >= 0 && i + expr.length <= to; i = text.indexOf(expr, i + expr.length)) out.push(i);
  return out;
}

function info(text: string, kind: IntroduceKind, s: number, e: number): IntroduceInfo {
  const expr = text.slice(s, e);
  const fn = enclosingFunction(text, s);
  const scope: [number, number] = kind === "constant" ? [0, text.length] : fn ? [fn.open, fn.close] : [0, text.length];
  const names = suggest(expr);
  const literal = /^(['"][^'"$]*['"]|\d+(\.\d+)?|true|false|null)$/i.test(expr.trim());
  return {
    range: rangeOf(text, s, e),
    text: expr.replace(/\s+/g, " "),
    occurrences: occurrencesOf(text, expr, scope[0], scope[1]).length,
    names: kind === "constant" ? names.map((n) => wordsOf(n).join("_").toUpperCase()) : names,
    className: /class\s+(\w+)/.exec(text)?.[1] ?? null,
    functionName: fn ? `${/class\s+(\w+)/.exec(text)?.[1] ?? ""}::${fn.name}()`.replace(/^::/, "") : null,
    initPlaces: literal ? ["currentMethod", "fieldDeclaration", "constructor"] : ["currentMethod"],
    isStatic: false,
    constantExpression: literal,
    global: !/class\s+\w+/.test(text),
    callSites: 1,
  };
}

registerMock("refactor_introduce_prepare", ({ path, kind, start, end, content }): IntroducePrepareAnswer => {
  const text = String(content);
  if (!isPhp(String(path))) throw new Error("This refactoring needs a PHP file or a language server that offers it");
  const k = kind as IntroduceKind;
  const s = offsetOf(text, start as LspPosition);
  const e = offsetOf(text, end as LspPosition);
  if (s === e) {
    const found = candidates(text, s);
    if (!found.length) throw new Error(NOT_EXPRESSION);
    if (found.length > 1)
      return {
        expressions: found.map(([a, b]) => ({
          range: rangeOf(text, a, b),
          text: text.slice(a, b).replace(/\s+/g, " "),
        })),
      };
    return { expressions: [], info: info(text, k, found[0][0], found[0][1]) };
  }
  const [a, b] = trimmed(text, s, e);
  if (a === b || /[;{}]/.test(text.slice(a, b))) throw new Error(NOT_EXPRESSION);
  if (k === "constant" && !info(text, k, a, b).constantExpression)
    throw new Error(`${CANNOT} Selected expression cannot be used as a constant initializer`);
  return { expressions: [], info: info(text, k, a, b) };
});

registerMock("refactor_introduce", async ({ path, kind, start, end, content, options }): Promise<RefactorAnswer> => {
  const text = String(content);
  const p = String(path);
  const o = options as IntroduceOptions;
  const name = o.name.replace(/^\$/, "");
  if (!/^[A-Za-z_]\w*$/.test(name)) throw new Error(`'${o.name}' is not a valid identifier`);
  const [s, e] = trimmed(text, offsetOf(text, start as LspPosition), offsetOf(text, end as LspPosition));
  const expr = text.slice(s, e);
  const fn = enclosingFunction(text, s);
  const k = kind as IntroduceKind;
  const scope: [number, number] = k === "constant" ? [0, text.length] : fn ? [fn.open, fn.close] : [0, text.length];
  const at = o.replaceAll ? occurrencesOf(text, expr, scope[0], scope[1]) : [s];
  const lineStart = (i: number) => text.lastIndexOf("\n", i - 1) + 1;
  const indentAt = (i: number) => /^[ \t]*/.exec(text.slice(lineStart(i)))![0];
  const first = Math.min(...at);
  const stmt = lineStart(first) + indentAt(first).length;
  const edits: Edit[] = [];
  const replaceWith = (ref: string) => at.forEach((i) => edits.push({ start: i, end: i + expr.length, text: ref }));
  const classOpen = text.indexOf("{", text.search(/class\s+\w+/));
  const member = indentAt(fn?.start ?? classOpen + 2) || "    ";
  switch (k) {
    case "variable":
      edits.push({
        start: stmt,
        end: stmt,
        text: `$${mark(name)} = ${expr};\n${indentAt(first)}`,
      });
      replaceWith(`$${mark(name)}`);
      break;
    case "constant": {
      const vis = o.visibility ? `${o.visibility} ` : "";
      edits.push({
        start: classOpen + 1,
        end: classOpen + 1,
        text: `\n${member}${vis}const ${mark(name)} = ${expr};\n`,
      });
      replaceWith(`self::${mark(name)}`);
      break;
    }
    case "field": {
      const vis = o.visibility ?? "private";
      const decl = o.init === "fieldDeclaration" ? `${vis} $${mark(name)} = ${expr};` : `${vis} $${mark(name)};`;
      edits.push({
        start: classOpen + 1,
        end: classOpen + 1,
        text: `\n${member}${decl}\n`,
      });
      if (o.init !== "fieldDeclaration")
        edits.push({
          start: stmt,
          end: stmt,
          text: `$this->${mark(name)} = ${expr};\n${indentAt(first)}`,
        });
      replaceWith(`$this->${mark(name)}`);
      break;
    }
    case "parameter": {
      if (!fn) throw new Error(`${CANNOT} The expression should be inside a function or method`);
      const close = text.lastIndexOf(")", text.lastIndexOf("{", fn.open + 1));
      const empty = /\(\s*$/.test(text.slice(fn.start, close));
      edits.push({
        start: close,
        end: close,
        text: `${empty ? "" : ", "}$${mark(name)}${o.defaultValue ? ` = ${expr}` : ""}`,
      });
      replaceWith(`$${mark(name)}`);
      break;
    }
  }
  return answer(p, text, edits);
});

// ---- Extract Method ----

const VAR = /\$(\w+)/g;

function vars(code: string): string[] {
  const out: string[] = [];
  for (const m of code.matchAll(VAR)) if (m[1] !== "this" && !out.includes(m[1])) out.push(m[1]);
  return out;
}

function fragment(text: string, s: number, e: number) {
  [s, e] = trimmed(text, s, e);
  const fn = enclosingFunction(text, s);
  if (s === e || !fn || e > fn.close) throw new Error(`${CANNOT} Selected block should represent a set of statements or an expression`);
  const code = text.slice(s, e);
  const statements = /;\s*$/.test(text.slice(s, e + 1)) || code.includes("\n");
  if (statements && text[e] === ";") e++;
  const body = text.slice(s, e);
  const before = text.slice(fn.start, s);
  const after = text.slice(e, fn.close);
  const assigned = [...body.matchAll(/\$(\w+)\s*=[^=>]/g)].map((m) => m[1]);
  const params = vars(body).filter((v) => new RegExp(`\\$${v}\\b`).test(before) && !assigned.includes(v));
  const outputs = [...new Set(assigned)].filter((v) => new RegExp(`\\$${v}\\b`).test(after));
  return {
    s,
    e,
    fn,
    body,
    statements,
    params,
    outputs,
    usesThis: body.includes("$this"),
  };
}

registerMock("refactor_extract_method_prepare", ({ path, start, end, content }): ExtractMethodInfo => {
  if (!isPhp(String(path))) throw new Error("This refactoring needs a PHP file or a language server that offers it");
  const text = String(content);
  const f = fragment(text, offsetOf(text, start as LspPosition), offsetOf(text, end as LspPosition));
  const inClass = /class\s+\w+/.test(text);
  return {
    title: inClass ? "Extract Method" : "Extract Function",
    range: rangeOf(text, f.s, f.e),
    name: f.statements ? "" : `get${camel(suggest(f.body)).replace(/^./, (c) => c.toUpperCase())}`,
    parameters: f.params.map((n) => ({
      name: n,
      typeHint: new RegExp(`(\\w+)\\s+\\$${n}\\b`).exec(text.slice(f.fn.start, f.fn.open))?.[1] ?? null,
    })),
    outputs: f.outputs,
    returns: !f.statements ? "expression" : f.outputs.length ? "variables" : "none",
    inClass,
    usesThis: f.usesThis,
    staticContext: false,
    duplicates: 0,
    className: /class\s+(\w+)/.exec(text)?.[1] ?? null,
  };
});

registerMock("refactor_extract_method", async ({ path, start, end, content, options }): Promise<RefactorAnswer> => {
  const text = String(content);
  const o = options as ExtractMethodOptions;
  if (!/^[A-Za-z_]\w*$/.test(o.name)) throw new Error(`'${o.name}' is not a valid identifier`);
  const f = fragment(text, offsetOf(text, start as LspPosition), offsetOf(text, end as LspPosition));
  const inClass = /class\s+\w+/.test(text);
  const params = o.parameters.length ? o.parameters : f.params.map((n) => ({ original: n, name: n }));
  const lineStart = text.lastIndexOf("\n", f.s - 1) + 1;
  const indent = text.slice(lineStart, f.s);
  let body = f.body;
  for (const p of params) if (p.name !== p.original) body = body.replace(new RegExp(`\\$${p.original}\\b`, "g"), `$${p.name}`);
  const inner = `${f.fn.indent}    `;
  const lines = f.statements
    ? body.split("\n").map((l, i) => (i ? `${inner}${l.slice(indent.length)}` : `${inner}${l}`))
    : [`${inner}return ${body};`];
  if (f.statements && f.outputs.length) {
    lines.push("", `${inner}return ${f.outputs.length === 1 ? `$${f.outputs[0]}` : `[${f.outputs.map((v) => `$${v}`).join(", ")}]`};`);
  }
  const args = params.map((p) => `$${p.original}`).join(", ");
  const typeOf = (n: string) => {
    const t = new RegExp(`(\\w+)\\s+\\$${n}\\b`).exec(text.slice(f.fn.start, f.fn.open))?.[1];
    return t ? `${t} ` : "";
  };
  const callee = inClass ? (o.isStatic ? `self::${mark(o.name)}` : `$this->${mark(o.name)}`) : mark(o.name);
  const call = `${callee}(${args})`;
  const replacement = !f.statements
    ? call
    : f.outputs.length === 0
      ? `${call};`
      : f.outputs.length === 1
        ? `$${f.outputs[0]} = ${call};`
        : `[${f.outputs.map((v) => `$${v}`).join(", ")}] = ${call};`;
  const vis = inClass ? `${o.visibility ?? "private"} ${o.isStatic ? "static " : ""}` : "";
  const decl = `\n\n${f.fn.indent}${vis}function ${mark(o.name)}(${params.map((p) => `${typeOf(p.original)}$${p.name}`).join(", ")})\n${f.fn.indent}{\n${lines.join("\n")}\n${f.fn.indent}}`;
  return answer(String(path), text, [
    { start: f.s, end: f.e, text: replacement },
    { start: f.fn.close + 1, end: f.fn.close + 1, text: decl },
  ]);
});

// ---- Inline ----

function inlineTarget(text: string, offset: number) {
  const word = /[$\w]+$/.exec(text.slice(0, offset))?.[0] ?? "";
  const rest = /^\w*/.exec(text.slice(offset))?.[0] ?? "";
  const name = word + rest;
  const start = offset - word.length;
  if (name.startsWith("$") && name !== "$this") {
    const fn = enclosingFunction(text, offset);
    const scope = fn ? text.slice(fn.open, fn.close) : text;
    const base = fn ? fn.open : 0;
    const def = new RegExp(`^([ \\t]*)\\${name}\\s*=\\s*([^;]+);[^\\n]*\\n`, "m").exec(scope);
    if (!def) throw new Error(`${CANNOT} Variable ${name} has no initializer`);
    const defStart = base + def.index;
    const defEnd = defStart + def[0].length;
    const refs = [...text.slice(defEnd, fn ? fn.close : text.length).matchAll(new RegExp(`\\${name}\\b`, "g"))].map(
      (m) => defEnd + m.index!,
    );
    if (!refs.length) throw new Error(`${CANNOT} Variable ${name} is never used`);
    return {
      kind: "variable" as const,
      name,
      start,
      value: def[2].trim(),
      defStart,
      defEnd,
      refs,
      onReference: refs.includes(start),
    };
  }
  const call = new RegExp(`(?:->|::)${name}\\s*\\(`).test(text.slice(start - 2, offset + rest.length + 2));
  const method = new RegExp(`function\\s+${name}\\s*\\(([^)]*)\\)[^{]*\\{\\s*return\\s+([^;]+);\\s*\\}`).exec(text);
  if (method && name) {
    const calls = [...text.matchAll(new RegExp(`(?:\\$this->|self::|static::)${name}\\(([^()]*)\\)`, "g"))];
    return {
      kind: "method" as const,
      name,
      start,
      method,
      calls,
      onReference: call,
    };
  }
  throw new Error(`${CANNOT} Caret should be positioned at a variable, method, function or constant`);
}

registerMock("refactor_inline_prepare", ({ path, position, content }): InlineInfo => {
  if (!isPhp(String(path))) throw new Error("This refactoring needs a PHP file or a language server that offers it");
  const text = String(content);
  const t = inlineTarget(text, offsetOf(text, position as LspPosition));
  if (t.kind === "variable")
    return {
      kind: "variable",
      title: "Inline Variable",
      name: t.name,
      onReference: t.onReference,
      occurrences: t.refs.length,
      readOnly: false,
    };
  return {
    kind: "method",
    title: "Inline Method",
    name: `${t.name}()`,
    onReference: t.onReference,
    occurrences: t.calls.length,
    readOnly: false,
  };
});

registerMock("refactor_inline", async ({ path, position, content, mode }): Promise<RefactorAnswer> => {
  const text = String(content);
  const m = mode as InlineMode;
  const t = inlineTarget(text, offsetOf(text, position as LspPosition));
  const edits: Edit[] = [];
  if (t.kind === "variable") {
    const value = /^[\w$>-]+(\([^()]*\))?$/.test(t.value) ? t.value : `(${t.value})`;
    const refs = m === "this" && t.onReference ? [t.start] : t.refs;
    refs.forEach((r) => edits.push({ start: r, end: r + t.name.length, text: value }));
    if (m === "all" || t.refs.length === 1) edits.push({ start: t.defStart, end: t.defEnd, text: "" });
  } else {
    const params = t.method[1]
      .split(",")
      .map((p) => /\$(\w+)/.exec(p)?.[1])
      .filter(Boolean) as string[];
    for (const c of t.calls) {
      const args = c[1].split(",").map((a) => a.trim());
      let body = t.method[2];
      params.forEach((p, i) => (body = body.replace(new RegExp(`\\$${p}\\b`, "g"), args[i] ?? "null")));
      edits.push({
        start: c.index!,
        end: c.index! + c[0].length,
        text: body,
      });
    }
    if (m === "all") {
      const s = text.lastIndexOf("\n", t.method.index!) + 1;
      edits.push({
        start: s - 1,
        end: t.method.index! + t.method[0].length,
        text: "",
      });
    }
  }
  return answer(String(path), text, edits);
});

// The demo has no language server refactorings for JS / TS.
registerMock("code_refactor_actions", () => []);
