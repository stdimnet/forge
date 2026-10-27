/**
 * GraphQL files (`.graphql`, `.gql`): problems and completion against the project's schema
 * (SDL files, the `schema` of `graphql.config.*` / `.graphqlrc*`, or an introspected
 * endpoint saved as SDL), computed in Rust with apollo-compiler (`graphql_validate`,
 * `graphql_complete`). "Get GraphQL Schema from Endpoint" introspects a URL and writes the
 * schema file. Queries run in the HTTP client as `GRAPHQL` requests.
 */
import { EditorState, type Extension } from "@codemirror/state";
import { ViewPlugin, type ViewUpdate } from "@codemirror/view";
import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { graphqlApi } from "@/ipc/webTooling";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { publishDiagnostics } from "@/features/lsp/store";
import { styleOption } from "@/features/lsp/completion";
import { toLsp } from "@/features/lsp/util";
import { registerEditorExtension } from "@/editor/extensions";
import { askName } from "@/features/contextMenu/dialogs";
import { openFile } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { isPluginActive } from "@/state/project";
import { registerCommand } from "@/state/commands";
import { balloonsFor } from "@/keymap";

const showBalloon = balloonsFor("GraphQL");
const SOURCE = "graphql";
const GQL = /\.(graphql|gql|graphqls)$/i;
const active = () => isPluginActive("forge.graphql");

function diagnostics(path: string): Extension {
  return ViewPlugin.define((view) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let seq = 0;
    const run = async () => {
      const id = ++seq;
      if (!active() || !getSetting<boolean>("graphql.validation", true)) return publishDiagnostics(path, [], SOURCE);
      const text = view.state.doc.toString();
      const problems = await graphqlApi.validate(path, text).catch(() => null);
      if (id !== seq || !problems || view.state.doc.toString() !== text) return;
      const doc = view.state.doc;
      const diags: EditorDiagnostic[] = problems.map((p) => ({
        range: { start: toLsp(doc, Math.min(p.from, doc.length)), end: toLsp(doc, Math.min(Math.max(p.to, p.from), doc.length)) },
        severity: "error",
        message: p.message,
        source: "GraphQL",
      }));
      publishDiagnostics(path, diags, SOURCE);
    };
    const schedule = (delay: number) => {
      clearTimeout(timer);
      timer = setTimeout(() => void run(), delay);
    };
    schedule(0);
    return {
      update(u: ViewUpdate) {
        if (u.docChanged) schedule(400);
      },
      destroy() {
        clearTimeout(timer);
        seq++;
        publishDiagnostics(path, [], SOURCE);
      },
    };
  });
}

const KIND: Record<string, string> = { field: "property", argument: "variable", type: "class", keyword: "keyword", enum: "enum" };

async function complete(ctx: CompletionContext): Promise<CompletionResult | null> {
  if (!active()) return null;
  const before = ctx.state.sliceDoc(Math.max(0, ctx.pos - 1), ctx.pos);
  if (!ctx.explicit && !/[\w{(\s.]/.test(before)) return null;
  const answer = await graphqlApi.complete(ctx.state.doc.toString(), ctx.pos).catch(() => null);
  if (!answer || ctx.aborted || !answer.items.length) return null;
  return {
    from: answer.from,
    options: answer.items.map((i, n) =>
      styleOption({ label: i.label, type: KIND[i.kind] ?? "text", detail: i.detail, info: i.documentation, boost: Math.max(1, 99 - n) }, { deprecated: i.deprecated }),
    ),
    validFor: /^\w*$/,
  };
}

registerEditorExtension(({ path }) => {
  if (!GQL.test(path)) return null;
  const data = [{ autocomplete: complete }];
  return [diagnostics(path), EditorState.languageData.of(() => data)];
});

/** Get GraphQL Schema from Endpoint: introspect and save `schema.graphql`. */
export async function introspectEndpoint(): Promise<void> {
  const info = await graphqlApi.info().catch(() => null);
  const endpoint = info?.config?.endpoints[0];
  const url = await askName({
    title: "Get GraphQL Schema from Endpoint",
    label: "Endpoint URL:",
    initial: endpoint?.url ?? getSetting<string>("graphql.endpoint", "") ?? "",
    confirm: "Introspect",
  });
  if (!url?.trim()) return;
  try {
    const file = await graphqlApi.introspect(url.trim(), endpoint?.url === url.trim() ? endpoint.headers : undefined, endpoint?.schemaFile ?? info?.schemaFiles[0]);
    showBalloon("Schema saved", `The schema of ${url.trim()} was written to ${file}.`);
    await openFile(file);
  } catch (err) {
    showBalloon("Introspection failed", String(err));
  }
}

registerCommand({ id: "GraphQL.Introspect", title: "Get GraphQL Schema from Endpoint", category: "Tools", run: () => void introspectEndpoint() });
