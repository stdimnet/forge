/**
 * WordPress editor support in PHP files: hook name completion inside `add_action('…')`,
 * `do_action('…')` and friends, Ctrl+B between a hook's registrations and invocations (and
 * from a callback to its function, from a WordPress function to the stubs), gutter icons,
 * Find Usages of a hook and the "Use Yoda condition" weak warning with its Alt+Enter fix.
 * Everything is computed in Rust (`phpforge-fw-wordpress`); this file only renders it.
 */
import { EditorState, type Extension } from "@codemirror/state";
import { ViewPlugin, type EditorView, type ViewUpdate } from "@codemirror/view";
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { Anchor, Zap } from "lucide-solid";
import { wordpressApi, type HookCompletionItem, type HookGutterMark } from "@/ipc/wordpress";
import type { ContextAction } from "@/ipc/codeintel";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { registerEditorExtension } from "@/editor/extensions";
import { registerLineMarkerProvider, type LineMarker } from "@/editor/lineMarkers";
import { registerDeclarationProvider } from "@/features/lsp/goto";
import { publishDiagnostics } from "@/features/lsp/store";
import { toLsp } from "@/features/lsp/util";
import { registerContextActionHandler, registerContextActionProvider } from "@/features/codeintel/intentions";
import { registerUsagesProvider } from "@/features/codeintel/usages";
import { chooseDeclarationAt } from "@/features/navigation/declarationChooser";
import { revealSymbol } from "@/features/php/editorViews";
import { getSetting } from "@/state/settings";
import { wordpressActive } from "./store";

const isPhp = (path: string) => /\.(php|inc|phtml)$/i.test(path) && !path.endsWith(".blade.php");

/** Cheap check before asking the backend: the caret is in a string after a hook function. */
const HOOK_CALL = /\b(add_action|add_filter|do_action(_ref_array|_deprecated)?|apply_filters(_ref_array|_deprecated)?|remove_(all_)?(action|filter)s?|has_(action|filter)|did_(action|filter)|doing_(action|filter))\s*\(\s*['"][^'"]*$/;

function toOption(item: HookCompletionItem, rank: number): Completion {
  return {
    label: item.label,
    type: item.kind === "filter" ? "function" : "event",
    detail: item.core ? `${item.kind} · WordPress core` : `${item.kind} · ${item.detail}`,
    info: item.info,
    boost: 99 - Math.min(rank, 98) / 2,
  };
}

function completionSource(path: string) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!wordpressActive() || !getSetting<boolean>("wordpress.hookCompletion", true)) return null;
    const before = ctx.state.sliceDoc(Math.max(0, ctx.pos - 300), ctx.pos);
    if (!HOOK_CALL.test(before)) return null;
    const list = await wordpressApi.complete(path, ctx.state.doc.toString(), ctx.pos).catch(() => null);
    if (!list || ctx.aborted || !list.items.length) return null;
    return { from: list.from, to: Math.max(list.to, ctx.pos), options: list.items.map(toOption), validFor: /^[\w\-./{}$]*$/ };
  };
}

// ---- Yoda conditions --------------------------------------------------------------------

/** The last hints per file, for Alt+Enter. */
const yodaHints = new Map<string, { from: number; to: number; fixTitle: string; replacement: string }[]>();

function yodaInspection(path: string): Extension {
  return ViewPlugin.define((view) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let seq = 0;
    const run = () => {
      const n = ++seq;
      if (!wordpressActive() || !getSetting<boolean>("wordpress.yodaConditions", true)) {
        yodaHints.delete(path);
        publishDiagnostics(path, [], "wordpress");
        return;
      }
      const text = view.state.doc.toString();
      void wordpressApi
        .inspect(path, text)
        .then((list) => {
          if (n !== seq || view.state.doc.toString() !== text) return;
          yodaHints.set(path, list);
          const doc = view.state.doc;
          const diags: EditorDiagnostic[] = list
            .filter((p) => p.to <= doc.length)
            .map((p) => ({
              range: { start: toLsp(doc, p.from), end: toLsp(doc, p.to) },
              severity: "weak",
              message: p.message,
              source: "WordPress",
              code: p.code,
            }));
          publishDiagnostics(path, diags, "wordpress");
        })
        .catch(() => {});
    };
    timer = setTimeout(run, 400);
    return {
      update(u: ViewUpdate) {
        if (!u.docChanged) return;
        clearTimeout(timer);
        timer = setTimeout(run, 600);
      },
      destroy() {
        clearTimeout(timer);
        yodaHints.delete(path);
        publishDiagnostics(path, [], "wordpress");
      },
    };
  });
}

registerContextActionProvider(async (path, view) => {
  if (!wordpressActive() || !isPhp(path)) return [];
  const pos = view.state.selection.main.head;
  const hint = (yodaHints.get(path) ?? []).find((h) => h.from <= pos && pos <= h.to);
  if (!hint) return [];
  const action: ContextAction = {
    title: hint.fixTitle,
    kind: "quickFix",
    source: "wordpress",
    isPreferred: true,
    needsResolve: false,
    action: "WordPress.FlipYoda",
    data: hint,
  };
  return [action];
});

registerContextActionHandler("WordPress.FlipYoda", (data, ctx) => {
  const h = data as { from: number; to: number; replacement: string };
  if (h.to > ctx.view.state.doc.length) return;
  ctx.view.dispatch({ changes: { from: h.from, to: h.to, insert: h.replacement }, userEvent: "input.fix" });
});

registerEditorExtension(({ path }): Extension | null => {
  if (!isPhp(path)) return null;
  const data = [{ autocomplete: completionSource(path) }];
  return [EditorState.languageData.of(() => data), yodaInspection(path)];
});

// ---- Navigation, usages and gutter icons -------------------------------------------------

registerDeclarationProvider(async (path, view, pos) => {
  if (!wordpressActive() || !isPhp(path)) return false;
  const targets = await wordpressApi.goto(path, view.state.doc.toString(), pos);
  if (!targets.length) return false;
  chooseDeclarationAt(view, pos, targets, { open: (t) => void revealSymbol(t.path, { line: t.line }) });
  return true;
});

registerUsagesProvider(async (path: string, view: EditorView, pos: number, scope) => {
  if (!wordpressActive() || !isPhp(path) || scope === "currentFile") return null;
  return wordpressApi.usages(path, view.state.doc.toString(), pos);
});

/** Code Vision above the line: "fired in 2 places", "3 callbacks". */
function visionText(m: HookGutterMark): string | undefined {
  const n = m.targets?.length ?? 0;
  if (!n) return undefined;
  if (m.kind === "registration") return n === 1 ? "fired in 1 place" : `fired in ${n} places`;
  return n === 1 ? "1 callback" : `${n} callbacks`;
}

registerLineMarkerProvider({
  appliesTo: isPhp,
  watch: wordpressActive,
  markers: async (path, text): Promise<LineMarker[]> => {
    if (!wordpressActive() || !getSetting<boolean>("wordpress.gutterIcons", true)) return [];
    const marks = await wordpressApi.gutter(path, text);
    return marks.map(
      (m): LineMarker => ({
        line: m.line,
        icon: m.kind === "registration" ? Anchor : Zap,
        title: m.title,
        className: m.kind === "registration" ? "marker-wp-registration" : "marker-wp-invocation",
        vision: visionText(m),
        onClick: (view, event) => {
          const targets = m.targets ?? [];
          if (targets.length === 1) void revealSymbol(targets[0].path, { line: targets[0].line });
          else if (targets.length > 1) {
            const pos = view.posAtCoords({ x: event.clientX, y: event.clientY }) ?? view.state.doc.line(m.line + 1).from;
            chooseDeclarationAt(view, pos, targets, { open: (t) => void revealSymbol(t.path, { line: t.line }) });
          }
        },
      }),
    );
  },
});
