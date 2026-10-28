/**
 * Razor (`.razor`, `.cshtml`) as a template data language, like Rider: the file keeps its
 * HTML highlighting and the Razor parts are a decoration layer on top, with C# colored by
 * the C# tokenizer: `@expr`, `@(…)`, `@{ … }`, `@code { … }`, `@if (…)` and the other
 * control blocks, directives (`@page`, `@inject`, …), directive attributes (`@onclick`,
 * `@bind-Value`) and `@* comments *@`. This is per-keystroke editor work, so it lives in
 * TypeScript; the component model it navigates (Blazor components and their parameters)
 * comes from Rust (`dotnet_razor_components`).
 *
 * Blazor: Go to Declaration (Ctrl+B) on `<Rating>` opens `Rating.razor`; completion offers
 * the components after `<` and their `[Parameter]`s inside the tag.
 */
import { StringStream, type StreamParser } from "@codemirror/language";
import { csharp } from "@codemirror/legacy-modes/mode/clike";
import { html } from "@codemirror/lang-html";
import { EditorState, Prec, RangeSetBuilder, type Extension, type Text } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { createResource, createRoot } from "solid-js";
import { registerLanguage } from "@/editor/languages";
import { registerEditorExtension } from "@/editor/extensions";
import { openFileAt } from "@/editor/views";
import { registerDeclarationProvider } from "@/features/lsp/goto";
import { dotnetWebApi, type RazorComponent } from "@/ipc/dotnetWeb";
import { onFileSaved } from "@/state/editor";
import { dotnetActive } from "../state";

// ---- Scanning ------------------------------------------------------------------------------------

export type RazorKind = "csharp" | "transition" | "directive" | "comment" | "attribute";

export interface RazorRegion {
  from: number;
  to: number;
  kind: RazorKind;
}

const DIRECTIVES = new Set([
  "page",
  "model",
  "using",
  "inject",
  "inherits",
  "implements",
  "namespace",
  "layout",
  "attribute",
  "typeparam",
  "rendermode",
  "addTagHelper",
  "removeTagHelper",
  "tagHelperPrefix",
  "preservewhitespace",
  "section",
]);
const BLOCKS = new Set(["code", "functions"]);
const CONTROL = new Set(["if", "foreach", "for", "while", "switch", "using", "lock", "do", "try"]);
const CONTINUATIONS = /^\s*(else\s+if|else|catch|finally|while)\b/;

const isIdent = (c: string | undefined) => !!c && /[\w]/.test(c);

/** The index after the bracket matching the one at `open` (strings and comments skipped). */
function balanced(text: string, open: number): number {
  const o = text[open]!;
  const c = o === "(" ? ")" : o === "[" ? "]" : "}";
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    const ch = text[i]!;
    if (ch === '"' || ch === "'") {
      const q = ch;
      for (i++; i < text.length && text[i] !== q && text[i] !== "\n"; i++) if (text[i] === "\\") i++;
      continue;
    }
    if (ch === "/" && text[i + 1] === "/") {
      while (i < text.length && text[i] !== "\n") i++;
      continue;
    }
    if (ch === "/" && text[i + 1] === "*") {
      const end = text.indexOf("*/", i + 2);
      i = end < 0 ? text.length : end + 1;
      continue;
    }
    if (ch === o) depth++;
    else if (ch === c && --depth === 0) return i + 1;
  }
  return text.length;
}

/** The end of an implicit expression: `Model.Books[0].Title`, `Format(x)`, `await Load()`. */
function implicitEnd(text: string, start: number): number {
  let i = start;
  if (text.startsWith("await ", i)) i += 6;
  for (;;) {
    while (isIdent(text[i])) i++;
    const ch = text[i];
    if (ch === "(" || ch === "[") {
      i = balanced(text, i);
      continue;
    }
    if (ch === "<" && /^<[A-Z]\w*>/.test(text.slice(i))) {
      // Generic method call: `@Html.Raw<T>(…)` is rare; `<T>` followed by `(`.
      const close = text.indexOf(">", i);
      if (text[close + 1] === "(") {
        i = close + 1;
        continue;
      }
    }
    if ((ch === "." || (ch === "?" && text[i + 1] === ".")) && isIdent(text[i + (ch === "?" ? 2 : 1)])) {
      i += ch === "?" ? 2 : 1;
      continue;
    }
    return i;
  }
}

/** The Razor parts of a file. Everything else is the data language (HTML). */
export function scanRazor(text: string): RazorRegion[] {
  const out: RazorRegion[] = [];
  const push = (from: number, to: number, kind: RazorKind) => to > from && out.push({ from, to, kind });
  let i = 0;
  while (i < text.length) {
    const at = text.indexOf("@", i);
    if (at < 0) break;
    const next = text[at + 1];
    // `a@b.com`, `@@`.
    if (isIdent(text[at - 1]) || next === "@") {
      i = at + (next === "@" ? 2 : 1);
      continue;
    }
    if (next === "*") {
      const end = text.indexOf("*@", at + 2);
      const to = end < 0 ? text.length : end + 2;
      push(at, to, "comment");
      i = to;
      continue;
    }
    if (next === "{" || next === "(") {
      const to = balanced(text, at + 1);
      push(at, at + 1, "transition");
      push(at + 1, to, "csharp");
      i = to;
      continue;
    }
    if (!isIdent(next)) {
      i = at + 1;
      continue;
    }
    let j = at + 1;
    while (isIdent(text[j]) || text[j] === "-" || (text[j] === ":" && isIdent(text[j + 1]))) j++;
    const word = text.slice(at + 1, j);
    const base = word.split(/[-:]/)[0]!;
    // Directive attributes inside a tag: `@onclick="…"`, `@bind-Value="…"`, `@ref="…"`.
    const lineStart = text.lastIndexOf("\n", at - 1) + 1;
    const before = text.slice(lineStart, at);
    const inTag = before.lastIndexOf("<") > before.lastIndexOf(">");
    // `Value="@book.Stars"`: an expression inside an attribute value.
    const inValue = ((before.slice(before.lastIndexOf("<")).match(/"/g) ?? []).length & 1) === 1;
    if (inTag && !inValue) {
      push(at, j, "attribute");
      const eq = /^\s*=\s*"/.exec(text.slice(j));
      if (eq) {
        const vStart = j + eq[0].length;
        const vEnd = text.indexOf('"', vStart);
        if (vEnd > vStart) push(text[vStart] === "@" ? vStart + 1 : vStart, vEnd, "csharp");
        i = vEnd < 0 ? text.length : vEnd + 1;
      } else i = j;
      continue;
    }
    if (BLOCKS.has(word)) {
      const brace = text.indexOf("{", j);
      if (brace >= 0 && /^\s*$/.test(text.slice(j, brace))) {
        const to = balanced(text, brace);
        push(at, j, "directive");
        push(brace, to, "csharp");
        i = to;
        continue;
      }
    }
    if (DIRECTIVES.has(word) && /^\s*$/.test(before)) {
      const eol = text.indexOf("\n", j);
      const end = eol < 0 ? text.length : eol;
      push(at, j, "directive");
      push(j, end, "csharp");
      i = end;
      continue;
    }
    if (CONTROL.has(base)) {
      push(at, at + 1, "transition");
      push(at + 1, j, "csharp");
      let k = j;
      // The condition, then the block's braces; markup inside the block stays data language.
      for (;;) {
        while (/\s/.test(text[k] ?? "")) k++;
        if (text[k] === "(") {
          const close = balanced(text, k);
          push(k, close, "csharp");
          k = close;
          continue;
        }
        if (text[k] !== "{") break;
        const close = balanced(text, k);
        push(k, k + 1, "csharp");
        // Nested Razor inside the block.
        for (const r of scanRazor(text.slice(k + 1, close - 1))) push(r.from + k + 1, r.to + k + 1, r.kind);
        push(close - 1, close, "csharp");
        k = close;
        const cont = CONTINUATIONS.exec(text.slice(k));
        if (!cont) break;
        push(k, k + cont[0].length, "csharp");
        k += cont[0].length;
      }
      i = k;
      continue;
    }
    const end = implicitEnd(text, at + 1);
    push(at, at + 1, "transition");
    push(at + 1, end, "csharp");
    i = Math.max(end, at + 1);
  }
  return out;
}

/** C# token styles of a region (`keyword`, `string`, `type`, …) with their offsets. */
export function csharpTokens(text: string, from: number, to: number): { from: number; to: number; style: string }[] {
  const parser = csharp as StreamParser<unknown>;
  const state = parser.startState!(4);
  const out: { from: number; to: number; style: string }[] = [];
  let offset = from;
  for (const line of text.slice(from, to).split("\n")) {
    const stream = new StringStream(line, 4, 4);
    while (!stream.eol()) {
      stream.start = stream.pos;
      const style = parser.token(stream, state);
      if (stream.pos === stream.start) stream.next();
      if (style) out.push({ from: offset + stream.start, to: offset + stream.pos, style: style.split(" ")[0]! });
    }
    offset += line.length + 1;
  }
  return out;
}

// ---- The overlay ---------------------------------------------------------------------------------

const MAX = 400_000;

function decorate(doc: Text): DecorationSet {
  if (doc.length > MAX) return Decoration.none;
  const text = doc.toString();
  const marks: { from: number; to: number; cls: string }[] = [];
  for (const r of scanRazor(text)) {
    if (r.kind !== "csharp") {
      marks.push({ from: r.from, to: r.to, cls: `cm-razor-${r.kind}` });
      continue;
    }
    marks.push({ from: r.from, to: r.to, cls: "cm-razor-code" });
    for (const t of csharpTokens(text, r.from, r.to)) marks.push({ from: t.from, to: t.to, cls: `cm-razor-${t.style}` });
  }
  // Outer (longer) marks first at the same position.
  marks.sort((a, b) => a.from - b.from || b.to - a.to);
  const builder = new RangeSetBuilder<Decoration>();
  for (const m of marks) if (m.to > m.from) builder.add(m.from, m.to, Decoration.mark({ class: m.cls }));
  return builder.finish();
}

const theme = EditorView.baseTheme({
  ".cm-razor-code": { color: "var(--fg)" },
  ".cm-razor-transition, .cm-razor-directive, .cm-razor-attribute": { color: "var(--syn-keyword)", fontWeight: "bold" },
  ".cm-razor-comment": { color: "var(--syn-comment)", fontStyle: "italic" },
  ".cm-razor-keyword": { color: "var(--syn-keyword)" },
  ".cm-razor-string, .cm-razor-string-2": { color: "var(--syn-string)" },
  ".cm-razor-number, .cm-razor-atom": { color: "var(--syn-number)" },
  ".cm-razor-type, .cm-razor-builtin": { color: "var(--syn-type)" },
  ".cm-razor-def": { color: "var(--syn-function)" },
  ".cm-razor-meta": { color: "var(--syn-attribute)" },
  ".cm-razor-operator": { color: "var(--syn-operator)" },
});

const overlay = ViewPlugin.define(
  (view) => {
    let decorations = decorate(view.state.doc);
    return {
      get decorations() {
        return decorations;
      },
      update(u: ViewUpdate) {
        if (u.docChanged) decorations = decorate(u.state.doc);
      },
    };
  },
  { decorations: (v) => v.decorations },
);

/** HTML with the Razor layer above it. */
export const razorLanguage = (): Extension => [html(), Prec.high(overlay), theme];

registerLanguage({ id: "razor", load: () => razorLanguage() });

// ---- Blazor components -------------------------------------------------------------------------------

const isRazor = (path: string) => /\.razor$/i.test(path);

const [components, { refetch }] = createRoot(() => createResource(
  () => (dotnetActive() ? true : null),
  () => dotnetWebApi.components().catch(() => [] as RazorComponent[]),
));
export { components as razorComponents };

if (typeof document !== "undefined")
  onFileSaved((path) => {
    if (/\.razor(\.cs)?$/i.test(path) && dotnetActive()) void refetch();
  });

const byName = (name: string) => (components() ?? []).find((c) => c.name === name);

/** The component tag name at a position (`<Rating`, `</Rating`, `<Shared.Rating`). */
export function tagAt(line: string, column: number): string | null {
  let s = column;
  while (s > 0 && /[\w.]/.test(line[s - 1]!)) s--;
  let e = column;
  while (e < line.length && /[\w.]/.test(line[e]!)) e++;
  if (s === e || s === 0) return null;
  const opener = line[s - 1] === "<" || (line[s - 1] === "/" && line[s - 2] === "<");
  const name = line.slice(s, e).split(".").pop()!;
  return opener && /^[A-Z]/.test(name) ? name : null;
}

registerDeclarationProvider(async (path, view, pos) => {
  if (!isRazor(path) && !/\.cshtml$/i.test(path)) return false;
  const line = view.state.doc.lineAt(pos);
  const name = tagAt(line.text, pos - line.from);
  if (!name) return false;
  const c = byName(name) ?? (await refetch())?.find((x) => x.name === name);
  if (!c) return false;
  await openFileAt(c.path, 1);
  return true;
});

/** The open tag (name and whether the cursor is past its name) before a position. */
export function openTag(before: string): { name: string; attrs: boolean } | null {
  const lt = before.lastIndexOf("<");
  if (lt < 0 || before.lastIndexOf(">") > lt) return null;
  const m = /^<([A-Za-z_][\w.]*)(\s[^"]*(?:"[^"]*"[^"]*)*)?$/.exec(before.slice(lt));
  if (!m) return null;
  return { name: m[1]!.split(".").pop()!, attrs: m[2] !== undefined };
}

function completion(ctx: CompletionContext): CompletionResult | null {
  const list = components() ?? [];
  if (!list.length) return null;
  const line = ctx.state.doc.lineAt(ctx.pos);
  const before = line.text.slice(0, ctx.pos - line.from);
  const tag = openTag(before);
  if (!tag) return null;
  const word = /[\w-]*$/.exec(before)![0];
  const from = ctx.pos - word.length;
  if (!tag.attrs) {
    if (!/^[A-Z]/.test(word) && !ctx.explicit) return null;
    return {
      from,
      options: list.map((c) => ({ label: c.name, type: "class", detail: c.namespace ?? c.path.split("/").slice(-2, -1)[0] ?? "", boost: 2 })),
      validFor: /^\w*$/,
    };
  }
  const c = list.find((x) => x.name === tag.name);
  if (!c) return null;
  const used = new Set([...before.matchAll(/\s([\w-]+)\s*=/g)].map((m) => m[1]));
  return {
    from,
    options: c.parameters
      .filter((p) => !p.cascading && !p.capturesUnmatched && !used.has(p.name))
      .map((p) => ({
        label: p.name,
        type: "property",
        detail: p.typeName,
        info: p.required ? "[EditorRequired] parameter" : "[Parameter]",
        boost: p.required ? 3 : 1,
        apply: (view: EditorView, _c: unknown, f: number, t: number) =>
          view.dispatch({ changes: { from: f, to: t, insert: `${p.name}=""` }, selection: { anchor: f + p.name.length + 2 } }),
      })),
    validFor: /^[\w-]*$/,
  };
}

const completionData = [{ autocomplete: completion }];

registerEditorExtension(({ path }) => (isRazor(path) ? EditorState.languageData.of(() => completionData) : null));
