/**
 * Building blocks shared by the simple language packs: the token vocabulary of the stream
 * parsers, indentation folding, documentation hovers, keyword completion and syntax
 * diagnostics that feed the Problems view.
 */
import { ensureSyntaxTree, foldService } from "@codemirror/language";
import { EditorState, type Extension } from "@codemirror/state";
import { hoverTooltip } from "@codemirror/view";
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { ViewPlugin, type ViewUpdate } from "@codemirror/view";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { publishDiagnostics, servers } from "@/features/lsp/store";
import { renderMarkdown } from "@/features/lsp/markdown";
import { h, toLsp } from "@/features/lsp/util";

export { streamLanguage, tokenTable } from "@/editor/tokens";

const indentOf = (text: string, tabSize: number): number => {
  let n = 0;
  for (const ch of text) {
    if (ch === " ") n++;
    else if (ch === "\t") n += tabSize - (n % tabSize);
    else break;
  }
  return n;
};

/** Fold a line over the following lines that are indented deeper than it. */
export function indentFolding(): Extension {
  return foldService.of((state, lineStart, lineEnd) => {
    const doc = state.doc;
    const line = doc.lineAt(lineStart);
    const text = line.text;
    if (!text.trim()) return null;
    const base = indentOf(text, state.tabSize);
    let end = -1;
    for (let n = line.number + 1; n <= doc.lines; n++) {
      const next = doc.line(n);
      if (!next.text.trim()) continue;
      if (indentOf(next.text, state.tabSize) <= base) break;
      end = next.to;
    }
    return end > lineEnd ? { from: lineEnd, to: end } : null;
  });
}

export interface DocEntry {
  /** Monospace headline, e.g. the parameter with its default. */
  title: string;
  /** Markdown. */
  doc: string;
}

/** Hover popup in the same style as the language server documentation. */
export function docDom(entry: DocEntry): HTMLElement {
  const root = h("div", "lsp-hover");
  const doc = h("div", "lsp-hover-doc");
  doc.append(h("pre", "lsp-hover-signature", entry.title));
  const body = h("div", "lsp-hover-body md");
  body.append(renderMarkdown(entry.doc));
  doc.append(body);
  root.append(doc);
  return root;
}

export interface HoverHit {
  from: number;
  to: number;
  dom: () => HTMLElement;
}

export function hoverDocs(lookup: (state: EditorState, pos: number) => HoverHit | null): Extension {
  return hoverTooltip(
    (view, pos) => {
      const hit = lookup(view.state, pos);
      return hit ? { pos: hit.from, end: hit.to, above: false, create: () => ({ dom: hit.dom() }) } : null;
    },
    { hoverTime: 350 },
  );
}

/** Words before the cursor matching `re`, as a completion range. */
export function wordBefore(ctx: CompletionContext, re: RegExp): { from: number; text: string } | null {
  const m = ctx.matchBefore(re);
  if (!m) return null;
  return { from: m.from, text: m.text };
}

export function completionResult(from: number, options: Completion[], validFor?: RegExp): CompletionResult {
  return { from, options, validFor };
}

/** Distinct matches of `re` group 1 in the document, for "names used in this file" lists. */
export function collectNames(state: EditorState, re: RegExp): string[] {
  const names = new Set<string>();
  for (const m of state.doc.toString().matchAll(re)) names.add(m[1]);
  return [...names];
}

export interface SyntaxProblem {
  from: number;
  to: number;
  message: string;
  severity?: EditorDiagnostic["severity"];
}

/**
 * Report syntax problems of a file in the Problems view and as squiggles. Skipped while a
 * language server for the file runs, because it reports the same errors (and more).
 */
export function syntaxDiagnostics(
  path: string,
  options: { source: string; serverIds?: string[]; delay?: number },
  check: (state: EditorState) => SyntaxProblem[],
): Extension {
  return ViewPlugin.define((view) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const serverActive = () =>
      servers().some(
        (s) => options.serverIds?.includes(s.id) && (s.state === "starting" || s.state === "ready" || s.state === "busy"),
      );
    const run = () => {
      timer = undefined;
      if (serverActive()) return;
      const doc = view.state.doc;
      const diags: EditorDiagnostic[] = check(view.state).map((p) => ({
        range: { start: toLsp(doc, p.from), end: toLsp(doc, p.to) },
        severity: p.severity ?? "error",
        message: p.message,
        source: options.source,
      }));
      publishDiagnostics(path, diags);
    };
    timer = setTimeout(run, 0);
    return {
      update(u: ViewUpdate) {
        if (!u.docChanged) return;
        clearTimeout(timer);
        timer = setTimeout(run, options.delay ?? 300);
      },
      destroy() {
        clearTimeout(timer);
        if (!serverActive()) publishDiagnostics(path, []);
      },
    };
  });
}

/** Syntax errors the (error tolerant) Lezer parser of a language marked in its tree. */
export function lezerProblems(state: EditorState, limit = 50): SyntaxProblem[] {
  const tree = ensureSyntaxTree(state, state.doc.length, 100);
  const out: SyntaxProblem[] = [];
  tree?.iterate({
    enter(node) {
      if (out.length >= limit) return false;
      if (!node.type.isError) return;
      const to = node.to > node.from ? node.to : Math.min(node.from + 1, state.doc.length);
      out.push({ from: Math.max(0, to > node.from ? node.from : to - 1), to, message: "Syntax error" });
      return false;
    },
  });
  return out;
}

/**
 * Fold the sections Symfony Flex writes into `.env` and `.gitignore`:
 * `###> vendor/package ###` … `###< vendor/package ###`.
 */
export function markerFolding(): Extension {
  return foldService.of((state, lineStart) => {
    const line = state.doc.lineAt(lineStart);
    const open = /^\s*###>\s*(.+?)\s*###\s*$/.exec(line.text);
    if (!open) return null;
    for (let n = line.number + 1; n <= state.doc.lines; n++) {
      const next = state.doc.line(n);
      const close = /^\s*###<\s*(.+?)\s*###\s*$/.exec(next.text);
      if (close?.[1] === open[1]) return { from: line.to, to: next.to };
    }
    return null;
  });
}
