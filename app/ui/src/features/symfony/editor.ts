/**
 * Symfony editor support: completion and Ctrl+B for route names, service ids, templates and
 * parameters in PHP, Twig and YAML; Twig tag/filter/function/test completion; line markers
 * for routes, rendered templates and console commands.
 */
import type { Extension } from "@codemirror/state";
import { EditorState } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { FileCode2, Globe, Play, Route as RouteIcon } from "lucide-solid";
import { symfonyApi, type SymfonyCompletionItem, type SymfonyCompletionKind } from "@/ipc/symfony";
import { registerEditorExtension } from "@/editor/extensions";
import { registerLineMarkerProvider, type LineMarker } from "@/editor/lineMarkers";
import { registerDeclarationProvider } from "@/features/lsp/goto";
import { revealSymbol } from "@/features/php/editorViews";
import { getSetting } from "@/state/settings";
import { runConsoleCommand, setActiveTab } from "./console";
import { showToolWindow } from "@/state/layout";
import { symfonyActive } from "./store";

const kindOf = (path: string): "php" | "twig" | "yaml" | null =>
  /\.twig$/i.test(path) ? "twig" : /\.php$/i.test(path) && !path.endsWith(".blade.php") ? "php" : /\.ya?ml$/i.test(path) ? "yaml" : null;

/** CodeMirror completion types with icons in the shared completion popup. */
const TYPE: Record<SymfonyCompletionKind, string> = {
  route: "reference",
  service: "module",
  template: "file",
  parameter: "constant",
  tag: "keyword",
  filter: "function",
  function: "function",
  test: "keyword",
  variable: "variable",
};

/** Insert `name()` with the caret inside, unless a `(` follows. */
function applyCall(name: string) {
  return (view: EditorView, _c: Completion, from: number, to: number) => {
    const has = view.state.sliceDoc(to, to + 1) === "(";
    view.dispatch({
      changes: { from, to, insert: has ? name : `${name}()` },
      selection: { anchor: from + name.length + (has ? 0 : 1) },
      userEvent: "input.complete",
    });
  };
}

const INFO_LABEL: Partial<Record<SymfonyCompletionKind, string>> = {
  route: "Controller",
  service: "Visibility",
  tag: "Provided by",
  filter: "Provided by",
  function: "Provided by",
  variable: "Provided by",
};

/** Small documentation popup next to the list (controller of a route, provider of a filter). */
function infoNode(item: SymfonyCompletionItem): Node {
  const root = document.createElement("div");
  root.className = "sf-completion-info";
  const label = document.createElement("span");
  label.className = "sf-completion-info-label";
  label.textContent = INFO_LABEL[item.kind] ?? "";
  const value = document.createElement("code");
  value.textContent = item.info ?? "";
  root.append(label, value);
  return root;
}

function toOption(item: SymfonyCompletionItem, rank: number): Completion {
  return {
    label: item.label,
    type: TYPE[item.kind],
    detail: item.detail,
    info: item.info ? () => infoNode(item) : undefined,
    apply: item.kind === "function" && item.label !== "app" ? applyCall(item.label) : (item.apply ?? item.label),
    // Symfony contexts are specific: rank them above generic word/keyword suggestions.
    boost: 99 - Math.min(rank, 98) / 2,
  };
}

/** Cheap check that the cursor may be in a Symfony reference, before asking the backend. */
function mayComplete(kind: "php" | "twig" | "yaml", before: string, docBefore: string): boolean {
  const quotes = (before.match(/(?<!\\)['"]/g) ?? []).length;
  if (kind === "twig") {
    const open = Math.max(docBefore.lastIndexOf("{{"), docBefore.lastIndexOf("{%"));
    return open >= 0 && !/(\}\}|%\})/.test(docBefore.slice(open));
  }
  return quotes % 2 === 1;
}

function completionSource(path: string, kind: "php" | "twig" | "yaml") {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!symfonyActive()) return null;
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    const docBefore = ctx.state.sliceDoc(Math.max(0, ctx.pos - 2000), ctx.pos);
    if (!mayComplete(kind, before, docBefore)) return null;
    if (kind === "twig" && !getSetting<boolean>("symfony.twigCompletion", true)) {
      // Only references (routes, templates) without Twig syntax completion.
      if (!/['"][^'"]*$/.test(before)) return null;
    }
    const list = await symfonyApi.complete(path, ctx.state.doc.toString(), ctx.pos).catch(() => null);
    if (!list || ctx.aborted || !list.items.length) return null;
    return {
      from: list.from,
      options: list.items.map(toOption),
      validFor: /^[\w.\-/@\\:]*$/,
    };
  };
}

registerEditorExtension(({ path }): Extension | null => {
  const kind = kindOf(path);
  if (!kind) return null;
  const data = [{ autocomplete: completionSource(path, kind) }];
  return EditorState.languageData.of(() => data);
});

registerDeclarationProvider(async (path, view, pos) => {
  if (!symfonyActive() || !kindOf(path)) return false;
  const targets = await symfonyApi.goto(path, view.state.doc.toString(), pos);
  const t = targets[0];
  if (!t) return false;
  await revealSymbol(t.path, { line: t.line });
  return true;
});

registerLineMarkerProvider({
  appliesTo: (path) => kindOf(path) === "php" || kindOf(path) === "twig",
  watch: symfonyActive,
  markers: async (path, text): Promise<LineMarker[]> => {
    if (!symfonyActive() || !getSetting<boolean>("symfony.gutterIcons", true)) return [];
    const marks = await symfonyApi.gutter(path, text);
    return marks.map((m): LineMarker => {
      const target = m.targets?.[0];
      switch (m.kind) {
        case "route":
          return {
            line: m.line,
            icon: Globe,
            title: m.title,
            className: "marker-route",
            onClick: () => {
              setActiveTab("routes");
              showToolWindow("bottom", "symfony");
            },
          };
        case "command":
          return { line: m.line, icon: Play, title: m.title, className: "marker-run", onClick: () => m.command && void runConsoleCommand(m.command) };
        case "controller":
          return { line: m.line, icon: RouteIcon, title: m.title, className: "marker-controller", onClick: () => target && void revealSymbol(target.path, { line: target.line }) };
        default:
          return { line: m.line, icon: FileCode2, title: m.title, className: "marker-template", onClick: () => target && void revealSymbol(target.path, { line: target.line }) };
      }
    });
  },
});
