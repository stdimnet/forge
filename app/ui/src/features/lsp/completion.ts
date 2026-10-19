/** Code completion backed by the language server, with PHPStorm-like kind icons. */
import type { Extension } from "@codemirror/state";
import { EditorState } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import {
  autocompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
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
import { buildHoverDom } from "./hoverDoc";
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

/** Short kind group used for colors. */
export function kindGroup(kind: string): string {
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

function toOption(path: string, item: EditorCompletionItem, rank: number): Completion {
  const callable = CALLABLE.has(item.kind) && (item.insertText ?? item.label) === item.label;
  const option: Completion = {
    label: item.label,
    displayLabel: callable ? `${item.label}()` : undefined,
    type: item.kind,
    detail: item.detail ?? undefined,
    apply: callable ? applyCall(item.label) : (item.insertText ?? item.label),
    boost: Math.max(-99, 99 - rank),
    info: async () => {
      let doc = item.documentation;
      if (!doc && item.raw) doc = (await lspApi.completionResolve(path, item.raw).catch(() => null))?.documentation;
      return doc ? buildHoverDom(doc) : null;
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
        `cm-lsp-option kind-${kindGroup(c.type ?? "text")}${deprecatedOptions.has(c) ? " cm-lsp-deprecated-option" : ""}`,
    }),
  ];
}
