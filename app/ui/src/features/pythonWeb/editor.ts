/**
 * Django / Flask / FastAPI in the editor, computed in Rust (`python_web_*`):
 *
 * - Go to Declaration (Ctrl+B, Ctrl+Click) on template names (`render(request, "x.html")`,
 *   `render_template`, `{% extends %}`, `{% include %}`) and URL names (`{% url 'a:b' %}`,
 *   `reverse()`, `redirect()`, `url_for()`); anything else goes to the language server;
 * - completion of template names, URL names and Flask endpoints inside those strings;
 * - gutter icons: template references, routed views (their URL), Celery tasks and, in a
 *   template, the views that render it;
 * - inspections: unresolved templates and URL names.
 */
import { EditorState, Prec, type Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { Cog, Globe, LayoutTemplate, Route, SquareArrowOutUpRight } from "lucide-solid";
import { pythonWebApi, type WebGutterMark, type WebLocation } from "@/ipc/pythonWeb";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { registerEditorExtension } from "@/editor/extensions";
import { registerLineMarkerProvider } from "@/editor/lineMarkers";
import { publishDiagnostics } from "@/features/lsp/store";
import { registerCompletionKind } from "@/features/lsp/completion";
import { activeView } from "@/features/lsp/navigation";
import { chooseDeclarationAt } from "@/features/navigation/declarationChooser";
import { commands, registerCommand } from "@/state/commands";
import { claimMouseGesture, editorMouseAction } from "@/keymap";
import { anyWebActive, reveal, webActive } from "./state";

const SOURCE = "pyforge-web";

registerCompletionKind("pyTemplate", LayoutTemplate, "type");
registerCompletionKind("pyUrl", Route, "function");

export const isPythonFile = (path: string) => /\.py$/i.test(path);
/** Templates the analysis reads: in a `templates/` (or `jinja2/`) folder, or a template extension. */
export const isTemplateFile = (path: string) =>
  /\.(djhtml|jinja2?|j2)$/i.test(path) || (/(^|\/)(templates|jinja2)\//.test(path) && /\.(html?|txt|xml)$/i.test(path));
const applies = (path: string) => isPythonFile(path) || isTemplateFile(path);

/** The caret is inside a string a template or URL name can be in (checked before asking Rust). */
export function mayReference(before: string, python: boolean): boolean {
  if (!/["'][^"'\n]*$/.test(before)) return false;
  if (python) return /\b(render\w*|get_template|select_template|\w*TemplateResponse|template_name|reverse(_lazy)?|redirect|resolve_url|url_for)\b[^"'\n]*["'][^"'\n]*$/.test(before);
  return /\{%-?\s*(extends|include|import|from|embed|url)\s+["'][^"'\n]*$/.test(before) || /url_for\(\s*["'][^"'\n]*$/.test(before);
}

function beforeCaret(view: EditorView, pos: number): string {
  const line = view.state.doc.lineAt(pos);
  // Include the rest of the word so a caret at the start of a name still counts.
  const tail = /^[\w./:-]*/.exec(line.text.slice(pos - line.from))![0];
  return line.text.slice(0, pos - line.from + Math.min(tail.length, 1));
}

async function open(view: EditorView, pos: number, targets: WebLocation[]): Promise<void> {
  if (targets.length === 1) await reveal(targets[0]);
  else
    chooseDeclarationAt(
      view,
      pos,
      targets.map((t) => ({ path: t.path, line: t.line, column: t.character })),
      { open: (t) => void reveal({ path: t.path, line: t.line, character: t.column ?? 0 }) },
    );
}

/** Go to the template or URL pattern at `pos`; false when there is none. */
export async function webGoto(path: string, view: EditorView, pos: number): Promise<boolean> {
  if (!anyWebActive() || !applies(path) || !mayReference(beforeCaret(view, pos), isPythonFile(path))) return false;
  const targets = await pythonWebApi.goto(path, view.state.doc.toString(), pos).catch(() => []);
  if (!targets.length) return false;
  await open(view, pos, targets);
  return true;
}

/** Ctrl+B reaches editors as the global `GotoDeclaration` action: templates first. */
export function wrapGotoDeclaration(): void {
  const previous = commands().get("GotoDeclaration");
  registerCommand({
    id: "GotoDeclaration",
    title: previous?.title ?? "Go to Declaration or Usages",
    category: previous?.category ?? "Navigate",
    run: async () => {
      const a = activeView();
      if (a && (await webGoto(a.path, a.view, a.view.state.selection.main.head))) return;
      await previous?.run();
    },
  });
}

function mouseGoto(path: string): Extension {
  return Prec.high(
    EditorView.domEventHandlers({
      mousedown(e, view) {
        if (e.button !== 0 || editorMouseAction(e) !== "GotoDeclaration" || !anyWebActive()) return false;
        const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
        if (pos == null || !mayReference(beforeCaret(view, pos), isPythonFile(path))) return false;
        e.preventDefault();
        claimMouseGesture();
        view.dispatch({ selection: { anchor: pos } });
        void webGoto(path, view, pos);
        return true;
      },
    }),
  );
}

function completionSource(path: string) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!webActive()) return null;
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    if (!mayReference(before, isPythonFile(path))) return null;
    const answer = await pythonWebApi.complete(path, ctx.state.doc.toString(), ctx.pos).catch(() => null);
    if (!answer || ctx.aborted || !answer.items.length) return null;
    return {
      from: answer.from,
      to: Math.max(answer.to, ctx.pos),
      // Inside these strings only names make sense: rank them above word and tag completions.
      options: answer.items.map((i) => ({
        label: i.label,
        detail: i.detail,
        type: i.kind === "template" ? "pyTemplate" : "pyUrl",
        boost: 50,
      })),
      validFor: /^[\w./:-]*$/,
    };
  };
}

registerEditorExtension(({ path }): Extension | null => {
  if (!applies(path)) return null;
  const data = [{ autocomplete: completionSource(path) }];
  return [EditorState.languageData.of(() => data), mouseGoto(path)];
});

// ---- Gutter and inspections ------------------------------------------------------------

const ICONS: Record<WebGutterMark["kind"], typeof Globe> = {
  template: LayoutTemplate,
  route: Globe,
  view: SquareArrowOutUpRight,
  task: Cog,
};

function publish(path: string, problems: { line: number; start: number; end: number; message: string; severity: string; code: string }[]): void {
  const diags: EditorDiagnostic[] = problems.map((p) => ({
    range: { start: { line: p.line, character: p.start }, end: { line: p.line, character: p.end } },
    severity: p.severity as EditorDiagnostic["severity"],
    message: p.message,
    source: "Django",
    code: p.code,
  }));
  publishDiagnostics(path, diags, SOURCE);
}

registerLineMarkerProvider({
  appliesTo: applies,
  watch: () => anyWebActive(),
  markers: async (path, text) => {
    if (!anyWebActive()) {
      publish(path, []);
      return [];
    }
    const marks = await pythonWebApi.marks(path, text).catch(() => null);
    publish(path, marks?.problems ?? []);
    return (marks?.gutter ?? []).map((m) => ({
      line: m.line,
      icon: ICONS[m.kind],
      title: m.tooltip,
      className: `marker-pyweb-${m.kind}`,
      onClick: (view: EditorView) => {
        const pos = view.state.doc.line(Math.min(m.line + 1, view.state.doc.lines)).from;
        void open(view, pos, m.targets);
      },
    }));
  },
});
