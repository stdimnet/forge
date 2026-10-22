/**
 * PHPStorm metadata and JetBrains attributes in the PHP editor: Forge's inspections
 * (deprecated symbols struck through, expected values, `#[Immutable]` writes, unused
 * `#[Pure]` results, unreachable code after exit points, the PSR-4 namespace check) and the
 * completion of expected arguments, return values and `#[ArrayShape]` keys. Everything is
 * computed by the PHP index in Rust (`php_inspect`, `php_complete`); quick-fixes ("Replace
 * with …", "Set namespace to …") come through Alt+Enter with the other PHP intentions.
 */
import { createEffect, createRoot, on } from "solid-js";
import { EditorState, type Extension } from "@codemirror/state";
import { ViewPlugin, type EditorView, type ViewUpdate } from "@codemirror/view";
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { KeyRound } from "lucide-solid";
import { phpMetaApi, type PhpCompletion, type PhpCompletionItem, type PhpProblem } from "@/ipc/phpMeta";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { publishDiagnostics } from "@/features/lsp/store";
import { registerCompletionKind } from "@/features/lsp/completion";
import { toLsp } from "@/features/lsp/util";
import { getSetting } from "@/state/settings";
import { isPluginActive } from "@/state/project";
import { lastStats } from "./status";

export const INSPECTIONS_KEY = "php.inspections.enabled";
const SOURCE = "php-inspections";

registerCompletionKind("key", KeyRound, "property");

const enabled = () => isPluginActive("forge.php") && getSetting<boolean>(INSPECTIONS_KEY, true) !== false;

/** Problems as editor diagnostics (UTF-16 offsets → LSP positions). */
export function toDiagnostics(doc: EditorState["doc"], problems: PhpProblem[]): EditorDiagnostic[] {
  const len = doc.length;
  return problems
    .filter((p) => p.start <= len)
    .map((p) => ({
      range: { start: toLsp(doc, p.start), end: toLsp(doc, Math.min(Math.max(p.end, p.start), len)) },
      severity: p.severity,
      message: p.message,
      source: "PHP",
      code: p.code,
      deprecated: p.deprecated,
      unnecessary: p.unnecessary,
    }));
}

/** Inspect after edits (debounced) and after the index changed. */
function inspections(path: string): Extension {
  return ViewPlugin.define((view) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let seq = 0;
    const run = async () => {
      timer = undefined;
      const id = ++seq;
      if (!enabled()) {
        publishDiagnostics(path, [], SOURCE);
        return;
      }
      const text = view.state.doc.toString();
      const problems = await phpMetaApi.inspect(path, text).catch(() => null);
      if (id !== seq || !problems) return;
      // Edited meanwhile: the next run publishes.
      if (view.state.doc.toString() !== text) return;
      publishDiagnostics(path, toDiagnostics(view.state.doc, problems), SOURCE);
    };
    const schedule = (delay: number) => {
      clearTimeout(timer);
      timer = setTimeout(() => void run(), delay);
    };
    schedule(300);
    // Again when the index changed (metadata files, attributes elsewhere) or the setting.
    const dispose = createRoot((d) => {
      createEffect(on([lastStats, enabled], () => schedule(0), { defer: true }));
      return d;
    });
    return {
      update(u: ViewUpdate) {
        if (u.docChanged) schedule(600);
      },
      destroy() {
        clearTimeout(timer);
        dispose();
        seq++;
        publishDiagnostics(path, [], SOURCE);
      },
    };
  });
}

/** Replace the answer's range (grown by what was typed since the request). */
function applyItem(item: PhpCompletionItem, answer: PhpCompletion, requestPos: number) {
  return (view: EditorView, _c: Completion, from: number, to: number) => {
    const start = Math.min(answer.from, from);
    const end = Math.max(to, answer.to + (to - requestPos));
    view.dispatch({
      changes: { from: start, to: Math.min(end, view.state.doc.length), insert: item.insert },
      selection: { anchor: start + item.insert.length },
      userEvent: "input.complete",
    });
  };
}

export function toOptions(answer: PhpCompletion, pos: number): Completion[] {
  return answer.items.map((item, i) => ({
    label: item.label,
    type: item.kind,
    detail: item.detail,
    // Expected values come first, in their declared order.
    boost: 99 - Math.min(i, 98),
    apply: applyItem(item, answer, pos),
  }));
}

/** Cheap check on the text before the caret: inside a call, a comparison or `[`. */
export function mayComplete(before: string): boolean {
  return /[([,|]\s*['"]?[\w\\:]*$/.test(before) || /[=!]==?\s*['"]?[\w\\:]*$/.test(before);
}

function completionSource(path: string) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!isPluginActive("forge.php")) return null;
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    if (!ctx.explicit && !mayComplete(before)) return null;
    const answer = await phpMetaApi.complete(path, ctx.pos, ctx.state.doc.toString()).catch(() => null);
    if (!answer || ctx.aborted || !answer.items.length) return null;
    return { from: Math.min(answer.from, ctx.pos), options: toOptions(answer, ctx.pos), validFor: /^[\w\\:]*$/ };
  };
}

/** The metadata / attribute support of one PHP editor. */
export function phpMetaEditor(path: string): Extension {
  // CodeMirror tracks sources by identity: create the source once per editor.
  const data = [{ autocomplete: completionSource(path) }];
  return [inspections(path), EditorState.languageData.of(() => data)];
}
