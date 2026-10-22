/** Code completion backed by the language server, with PHPStorm-like kind icons. */
import type { Extension } from "@codemirror/state";
import { EditorState } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import {
  autocompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
  snippet,
} from "@codemirror/autocomplete";
import {
  Box,
  Braces,
  CircleDot,
  Component,
  FileCode,
  Folder,
  KeyRound,
  ListOrdered,
  Package,
  Pi,
  Plus,
  Scissors,
  SquareFunction,
  Text,
  Type,
  Variable,
  Zap,
} from "lucide-solid";
import { lspApi, type EditorCompletionItem } from "@/ipc/lsp";
import { buildHoverDom, hoverDocOf } from "./hoverDoc";
import { syncNow } from "./sync";
import { fromLsp, h, iconNode, toLsp } from "./util";

const kindIcons: Record<string, Parameters<typeof iconNode>[0]> = {
  method: SquareFunction,
  function: SquareFunction,
  constructor: SquareFunction,
  field: CircleDot,
  property: CircleDot,
  variable: Variable,
  class: Box,
  struct: Box,
  interface: Component,
  enum: ListOrdered,
  enumMember: Pi,
  constant: Pi,
  value: Pi,
  keyword: KeyRound,
  module: Package,
  snippet: Scissors,
  file: FileCode,
  folder: Folder,
  typeParameter: Type,
  event: Zap,
  operator: Plus,
  reference: Braces,
};

const extraGroups: Record<string, string> = {};

/**
 * Let other completion sources (e.g. a framework plugin) use their own kinds with an icon
 * and a color group (`function`, `property`, `type`, `constant`, `variable`, `keyword`).
 */
export function registerCompletionKind(kind: string, icon: Parameters<typeof iconNode>[0], group: string): void {
  kindIcons[kind] = icon;
  extraGroups[kind] = group;
}

/** Short kind group used for colors. */
export function kindGroup(kind: string): string {
  if (extraGroups[kind]) return extraGroups[kind];
  if (["method", "function", "constructor"].includes(kind)) return "function";
  if (["field", "property"].includes(kind)) return "property";
  if (["class", "struct", "interface", "enum", "typeParameter", "module"].includes(kind)) return "type";
  if (["enumMember", "constant", "value"].includes(kind)) return "constant";
  if (kind === "variable") return "variable";
  if (kind === "keyword") return "keyword";
  return "other";
}

export function kindIconNode(kind: string): Element {
  const span = h("span", `cm-lsp-kind kind-${kindGroup(kind)}`);
  span.append(iconNode(kindIcons[kind] ?? Text, 14));
  span.title = kind;
  return span;
}

/** PHP identifiers, variables and namespaced names. */
const WORD = /[$\w\\]*$/;
const TRIGGERS = ["->", "::", "?->", "$", "\\"];

const deprecatedOptions = new WeakSet<Completion>();
const extraClasses = new WeakMap<Completion, string>();

/**
 * Style an option of another completion source like the server's: struck through when
 * deprecated, plus extra classes (e.g. `cm-json-required` for required JSON properties).
 */
export function styleOption(option: Completion, opts: { deprecated?: boolean; class?: string }): Completion {
  if (opts.deprecated) deprecatedOptions.add(option);
  if (opts.class) extraClasses.set(option, opts.class);
  return option;
}
const CALLABLE = new Set(["method", "function", "constructor"]);

/** Insert `name()` with the caret between the parentheses (unless a `(` already follows). */
function applyCall(name: string) {
  return (view: EditorView, _c: Completion, from: number, to: number) => {
    const hasParen = view.state.sliceDoc(to, to + 1) === "(";
    const insert = hasParen ? name : `${name}()`;
    view.dispatch({
      changes: { from, to, insert },
      selection: { anchor: from + name.length + (hasParen ? 0 : 1) },
      userEvent: "input.complete",
    });
  };
}

const SNIPPET_FORMAT = 2;

/** LSP snippet syntax to CodeMirror's: `$1` becomes `${1}`, the final stop `$0` an empty field. */
export function toCodeMirrorSnippet(text: string): string {
  return text.replace(/\$\{0\}|\$0/g, "${}").replace(/\$(\d+)/g, "${$1}");
}

/** Apply the server's `textEdit`: its range may reach past the caret (e.g. the closing quote of a JSON key). */
function applyEdit(item: EditorCompletionItem) {
  const raw = item.raw as { insertTextFormat?: number } | null | undefined;
  const isSnippet = raw?.insertTextFormat === SNIPPET_FORMAT;
  const text = item.insertText ?? item.label;
  return (view: EditorView, completion: Completion, from: number, to: number) => {
    const end = item.replaceRange ? Math.max(fromLsp(view.state.doc, item.replaceRange.end), to) : to;
    if (isSnippet) return snippet(toCodeMirrorSnippet(text))(view, completion, from, end);
    view.dispatch({
      changes: { from, to: end, insert: text },
      selection: { anchor: from + text.length },
      userEvent: "input.complete",
    });
  };
}

export function toOption(path: string, item: EditorCompletionItem, rank: number): Completion {
  const callable = CALLABLE.has(item.kind) && (item.insertText ?? item.label) === item.label;
  // Options are matched on `label`; servers whose edit replaces more than the word (JSON keys
  // include the quotes) send the text to match as `filterText`.
  const filter = item.filterText && item.filterText !== item.label ? item.filterText : undefined;
  const needsEdit = item.replaceRange || (item.raw as { insertTextFormat?: number } | null)?.insertTextFormat === SNIPPET_FORMAT;
  const option: Completion = {
    label: filter ?? item.label,
    displayLabel: callable ? `${item.label}()` : filter ? item.label : undefined,
    type: item.kind,
    detail: item.detail ?? undefined,
    apply: callable ? applyCall(item.label) : needsEdit ? applyEdit(item) : (item.insertText ?? item.label),
    boost: Math.max(-99, 99 - rank),
    info: async () => {
      let doc = item.documentation;
      if (!doc && item.raw) doc = (await lspApi.completionResolve(path, item.raw).catch(() => null))?.documentation;
      const parsed = await hoverDocOf(doc);
      return parsed ? buildHoverDom(parsed) : null;
    },
  };
  if (item.deprecated) deprecatedOptions.add(option);
  return option;
}

export function lspCompletionSource(path: string) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    const word = WORD.exec(before)![0];
    const prefixEnd = before.length - word.length;
    const triggered = TRIGGERS.some((t) => before.slice(0, prefixEnd).endsWith(t) || word.startsWith(t));
    if (!ctx.explicit && !word && !triggered) return null;

    await syncNow(path);
    const list = await lspApi.completion(path, toLsp(ctx.state.doc, ctx.pos)).catch(() => null);
    if (!list || ctx.aborted || !list.items.length) return null;

    const sorted = [...list.items].sort((a, b) => (a.sortText ?? a.label).localeCompare(b.sortText ?? b.label));
    const replace = sorted.find((i) => i.replaceRange)?.replaceRange;
    const from = replace ? Math.min(fromLsp(ctx.state.doc, replace.start), ctx.pos) : ctx.pos - word.length;
    return {
      from,
      options: sorted.map((item, i) => toOption(path, item, i)),
      validFor: list.isIncomplete ? undefined : /^[$\w\\]*$/,
    };
  };
}

export function lspCompletion(path: string): Extension {
  // CodeMirror tracks sources by identity, so create the source (and its data) once.
  const data = [{ autocomplete: lspCompletionSource(path) }];
  return [
    EditorState.languageData.of(() => data),
    autocompletion({
      icons: false,
      addToOptions: [{ render: (c) => kindIconNode(c.type ?? "text"), position: 20 }],
      optionClass: (c) =>
        `cm-lsp-option kind-${kindGroup(c.type ?? "text")}${deprecatedOptions.has(c) ? " cm-lsp-deprecated-option" : ""}${
          extraClasses.has(c) ? ` ${extraClasses.get(c)}` : ""
        }`,
    }),
  ];
}
