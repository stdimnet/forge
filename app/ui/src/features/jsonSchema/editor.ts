/**
 * JSON Schema in the editor of JSON and YAML files: problems (PHPStorm's "Compliance with
 * JSON schema" inspection, warnings and struck-through deprecations), completion of
 * property names and values, and documentation on hover. Everything is computed in Rust
 * (`json_schema_*`); this module renders the answers.
 */
import { EditorState, type Extension } from "@codemirror/state";
import { hoverTooltip, ViewPlugin, type EditorView, type ViewUpdate } from "@codemirror/view";
import { snippet, startCompletion, type Completion, type CompletionContext, type CompletionResult } from "@codemirror/autocomplete";
import { jsonSchemaApi, type SchemaCompletion, type SchemaCompletionItem } from "@/ipc/jsonSchema";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { publishDiagnostics } from "@/features/lsp/store";
import { lspClaimsHover } from "@/features/lsp/hover";
import { styleOption, toCodeMirrorSnippet } from "@/features/lsp/completion";
import { toLsp } from "@/features/lsp/util";
import { docDom } from "@/features/languages/shared";
import { active, onSchemasChanged, OWN_COMPLETION, serverRuns, setSchemaInfo } from "./state";

const SOURCE = "json-schema";

/** Validate after edits and when mappings or schema files change. */
function schemaDiagnostics(path: string, language: string): Extension {
  return ViewPlugin.define((view) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let seq = 0;
    const run = async () => {
      timer = undefined;
      const id = ++seq;
      if (!active()) {
        setSchemaInfo(path, null);
        publishDiagnostics(path, [], SOURCE);
        return;
      }
      const text = view.state.doc.toString();
      const answer = await jsonSchemaApi.validate(path, language, text).catch(() => null);
      if (id !== seq || !answer) return;
      setSchemaInfo(path, answer.schema);
      // Edited meanwhile: the next run publishes.
      if (view.state.doc.toString() !== text) return;
      const doc = view.state.doc;
      const diags: EditorDiagnostic[] = serverRuns(language)
        ? []
        : answer.problems.map((p) => ({
            range: { start: toLsp(doc, p.from), end: toLsp(doc, Math.max(p.to, p.from)) },
            severity: p.severity,
            message: p.message,
            source: "JSON Schema",
          }));
      publishDiagnostics(path, diags, SOURCE);
    };
    const schedule = (delay: number) => {
      clearTimeout(timer);
      timer = setTimeout(() => void run(), delay);
    };
    schedule(0);
    const off = onSchemasChanged(() => schedule(100));
    return {
      update(u: ViewUpdate) {
        if (u.docChanged) schedule(400);
      },
      destroy() {
        clearTimeout(timer);
        seq++;
        off();
        publishDiagnostics(path, [], SOURCE);
      },
    };
  });
}

/** Replace the answer's range (grown by what was typed since the request) with the snippet. */
function applyItem(item: SchemaCompletionItem, answer: SchemaCompletion, requestPos: number) {
  return (view: EditorView, completion: Completion, _from: number, to: number) => {
    const end = Math.max(to, answer.replaceTo + (to - requestPos));
    snippet(toCodeMirrorSnippet(item.insert))(view, completion, answer.replaceFrom, Math.min(end, view.state.doc.length));
    if (item.retrigger) setTimeout(() => startCompletion(view), 0);
  };
}

function toOption(item: SchemaCompletionItem, rank: number, answer: SchemaCompletion, pos: number): Completion {
  return styleOption(
    {
      label: item.label,
      type: item.kind,
      detail: item.detail,
      boost: Math.max(-99, 99 - rank),
      apply: applyItem(item, answer, pos),
      info: item.documentation
        ? () => docDom({ title: item.detail ? `${item.label}: ${item.detail}` : item.label, doc: item.documentation! })
        : undefined,
    },
    { deprecated: item.deprecated, class: item.required ? "cm-json-required" : undefined },
  );
}

function completionSource(path: string, language: string) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!active() || serverRuns(language)) return null;
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    // While typing: after a word character or an opening quote; Ctrl+Space anywhere.
    if (!ctx.explicit && !/[\w"'$.\-/@]$/.test(before)) return null;
    const answer = await jsonSchemaApi.complete(path, language, ctx.state.doc.toString(), ctx.pos).catch(() => null);
    if (!answer || ctx.aborted || !answer.items.length) return null;
    return {
      from: answer.from,
      options: answer.items.map((item, i) => toOption(item, i, answer, ctx.pos)),
      validFor: /^[\w$.\-/@]*$/,
    };
  };
}

function schemaHover(path: string, language: string): Extension {
  return hoverTooltip(
    async (view, pos) => {
      if (!active() || serverRuns(language)) return null;
      if (await lspClaimsHover(path, view, pos)) return null;
      const hover = await jsonSchemaApi.hover(path, language, view.state.doc.toString(), pos).catch(() => null);
      if (!hover) return null;
      return {
        pos: hover.from,
        end: hover.to,
        above: false,
        create: () => ({ dom: docDom({ title: hover.title, doc: hover.markdown }) }),
      };
    },
    { hoverTime: 350 },
  );
}

/** The JSON Schema support of one editor. */
export function jsonSchemaEditor(path: string, language: string): Extension {
  const out: Extension[] = [schemaDiagnostics(path, language)];
  // Compose and Symfony services have their own (richer) key completion and docs.
  if (!OWN_COMPLETION.has(language)) {
    // CodeMirror tracks sources by identity: create the source once per editor.
    const data = [{ autocomplete: completionSource(path, language) }];
    out.push(EditorState.languageData.of(() => data), schemaHover(path, language));
  }
  return out;
}
