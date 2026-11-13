/**
 * Laravel completion, added next to the language server's: route names, views, config and
 * env keys, translations, query columns and relations, model properties and scopes, Blade
 * components and directives, and the values of `ide.json` completions. Items use the shared completion popup (icons via
 * `registerCompletionKind`).
 */
import { EditorState, type Extension } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { AtSign, Component, Database, KeyRound, Languages, LayoutTemplate, Link, Route, Settings2 } from "lucide-solid";
import { laravelApi, type LaravelCompletionItem } from "@/ipc/laravel";
import { registerCompletionKind } from "@/features/lsp/completion";
import { h } from "@/features/lsp/util";
import { BLADE_DIRECTIVES, type BladeDirective } from "./blade";
import { laravelActive, laravelSetting } from "./state";

registerCompletionKind("route", Route, "function");
registerCompletionKind("view", LayoutTemplate, "type");
registerCompletionKind("config", Settings2, "constant");
registerCompletionKind("env", KeyRound, "constant");
registerCompletionKind("translation", Languages, "other");
registerCompletionKind("column", Database, "property");
registerCompletionKind("relation", Link, "function");
registerCompletionKind("component", Component, "type");
registerCompletionKind("directive", AtSign, "keyword");

export const isBlade = (path: string) => path.endsWith(".blade.php");

/**
 * Cheap check on the text before the caret, so the backend is only asked where Laravel
 * completion can apply.
 */
export function mayComplete(before: string, blade: boolean): boolean {
  if (/[@\w]+\s*\([^()'"]*(?:\([^()]*\)[^()'"]*)*['"][^'"]*$/.test(before)) return true; // string argument
  if (/\$\w+(?:\??->\w+)*\??->\w*$/.test(before)) return true; // $model->prop
  if (/\b[A-Z]\w*::\w*$/.test(before)) return true; // Model::scope
  if (/\[\s*\w+::class\s*,\s*['"][^'"]*$/.test(before)) return true; // [Controller::class, 'method']
  if (blade && /<\/?(?:x-|livewire:)?[\w.:-]*$/.test(before)) return true;
  if (blade && /(^|[^\w@])@\w*$/.test(before)) return true;
  return false;
}

/** `@foreach` → `@foreach(|)` + `@endforeach` on its own line. */
function applyDirective(d: BladeDirective) {
  return (view: EditorView, _c: Completion, from: number, to: number) => {
    const line = view.state.doc.lineAt(from);
    const indent = /^\s*/.exec(line.text)![0];
    const hasParen = view.state.sliceDoc(to, to + 1) === "(";
    let insert = `@${d.name}`;
    let caret = insert.length;
    if (d.args && !hasParen) {
      insert += "()";
      caret = insert.length - 1;
    }
    const block = d.end && line.text.slice(0, from - line.from).trim() === "";
    if (block) {
      insert += `\n${indent}    \n${indent}@${d.end}`;
      if (!d.args) caret = insert.indexOf("\n") + 1 + indent.length + 4;
    }
    view.dispatch({ changes: { from, to, insert }, selection: { anchor: from + caret }, userEvent: "input.complete" });
  };
}

function directiveOptions(custom: string[]): Completion[] {
  const builtins = BLADE_DIRECTIVES.map(
    (d): Completion => ({
      label: `@${d.name}`,
      type: "directive",
      detail: d.description,
      apply: applyDirective(d),
      boost: d.name.startsWith("end") ? -10 : 0,
    }),
  );
  const extra = custom
    .filter((name) => !BLADE_DIRECTIVES.some((d) => d.name === name))
    .map((name): Completion => ({ label: `@${name}`, type: "directive", detail: "custom directive", apply: applyDirective({ name, args: true, description: "" }) }));
  return [...builtins, ...extra];
}

function toOption(item: LaravelCompletionItem, index: number): Completion {
  return {
    label: item.label,
    type: item.kind,
    detail: item.detail,
    apply: item.insertText ?? item.label,
    boost: 50 - Math.min(index, 49) / 100,
    info: item.documentation ? () => h("div", "cm-laravel-doc", item.documentation!) : undefined,
  };
}

export function laravelCompletionSource(path: string) {
  const blade = isBlade(path);
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!laravelActive() || !laravelSetting("laravel.completion.enabled", true)) return null;
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    if (!mayComplete(before, blade)) return null;
    const result = await laravelApi.complete(path, ctx.pos, ctx.state.doc.toString()).catch(() => null);
    if (ctx.aborted) return null;
    let options = result?.items.map(toOption) ?? [];
    if (!laravelSetting("laravel.completion.columns", true)) {
      options = options.filter((o) => o.type !== "column");
    }
    if (!laravelSetting("laravel.completion.ideJson", true)) {
      options = options.filter((o) => o.type !== "value");
    }
    let from = result ? Math.min(result.from, ctx.pos) : ctx.pos;
    // Blade directives: built-ins plus the project's custom directives.
    const directive = blade ? /(^|[^\w@])(@\w*)$/.exec(before) : null;
    if (directive && !/['"]/.test(before.slice(before.length - directive[2].length - 1, before.length - directive[2].length))) {
      const custom = [
        ...options.filter((o) => o.type === "directive").map((o) => o.label.slice(1)),
        ...laravelSetting<string[]>("laravel.blade.customDirectives", []),
      ];
      options = [...options.filter((o) => o.type !== "directive"), ...directiveOptions(custom)];
      from = ctx.pos - directive[2].length;
    }
    if (!options.length) return null;
    return { from, options, validFor: /^[@\w.:-]*$/ };
  };
}

/** The Laravel source next to the LSP one (CodeMirror merges sources). */
export function laravelCompletion(path: string): Extension {
  const data = [{ autocomplete: laravelCompletionSource(path) }];
  return EditorState.languageData.of(() => data);
}
