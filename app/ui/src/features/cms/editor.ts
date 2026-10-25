/**
 * Drupal hooks in .module, .install, .theme, .profile and .inc files: completion of
 * `<module>_<hook>` after `function` (with the hook's parameters), a gutter icon on each
 * implementation and Ctrl+B to the hook's documentation (its `hook_…` function in the
 * project's *.api.php files, else api.drupal.org). Computed in Rust (`phpforge-fw-cms`).
 */
import { EditorState, type Extension } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { BookOpen } from "lucide-solid";
import { cmsApi, type DrupalHookCompletion, type DrupalHookDocs } from "@/ipc/cms";
import { registerEditorExtension } from "@/editor/extensions";
import { registerLineMarkerProvider, type LineMarker } from "@/editor/lineMarkers";
import { registerDeclarationProvider } from "@/features/lsp/goto";
import { revealSymbol } from "@/features/php/editorViews";
import { getSetting } from "@/state/settings";
import { drupalActive } from "./store";

const isHookFile = (path: string) => /\.(module|install|theme|profile|inc|engine|php)$/i.test(path) && !/\.(api|blade)\.php$/i.test(path);

/** Open a hook's documentation: the project's API file, else its page on api.drupal.org. */
export function openHookDocs(docs: DrupalHookDocs): void {
  if (docs.location) void revealSymbol(docs.location.path, { line: docs.location.line });
  else window.open(docs.url, "_blank", "noopener");
}

/** Insert `acme_form_alter(&$form, …) {\n  \n}` with the caret in the body. */
function applyHook(item: DrupalHookCompletion) {
  return (view: EditorView, _c: Completion, from: number, to: number) => {
    const rest = view.state.sliceDoc(to, to + 1);
    const head = rest === "(" ? item.label : `${item.label}(${item.params}) {\n  `;
    const insert = rest === "(" ? head : `${head}\n}`;
    view.dispatch({
      changes: { from, to, insert },
      selection: { anchor: from + head.length },
      userEvent: "input.complete",
    });
  };
}

function completionSource(path: string) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!drupalActive() || !getSetting<boolean>("cms.drupal.hookCompletion", true)) return null;
    const line = ctx.state.doc.lineAt(ctx.pos);
    if (!/^\s*function\s+\w*$/.test(line.text.slice(0, ctx.pos - line.from))) return null;
    const list = await cmsApi.complete(path, ctx.state.doc.toString(), ctx.pos).catch(() => null);
    if (!list || ctx.aborted || !list.items.length) return null;
    return {
      from: list.from,
      options: list.items.map((i) => ({
        label: i.label,
        type: "function",
        detail: i.hook,
        info: i.summary,
        apply: applyHook(i),
        boost: 50,
      })),
      validFor: /^\w*$/,
    };
  };
}

registerEditorExtension(({ path }): Extension | null => {
  if (!isHookFile(path)) return null;
  const data = [{ autocomplete: completionSource(path) }];
  return EditorState.languageData.of(() => data);
});

registerDeclarationProvider(async (path, view, pos) => {
  if (!drupalActive() || !isHookFile(path)) return false;
  const docs = await cmsApi.goto(path, view.state.doc.toString(), pos);
  if (!docs) return false;
  openHookDocs(docs);
  return true;
});

registerLineMarkerProvider({
  appliesTo: isHookFile,
  watch: drupalActive,
  markers: async (path, text): Promise<LineMarker[]> => {
    if (!drupalActive() || !getSetting<boolean>("cms.drupal.gutterIcons", true)) return [];
    const marks = await cmsApi.gutter(path, text);
    return marks.map((m) => ({
      line: m.line,
      icon: BookOpen,
      title: `${m.title}. Click to open the hook docs`,
      className: "marker-drupal-hook",
      onClick: () => openHookDocs(m.docs),
    }));
  },
});
