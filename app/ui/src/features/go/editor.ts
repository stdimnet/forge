/**
 * GoForge in the editor:
 *
 * - `go.mod`, `go.work` and `go.sum` highlighting, problems of their directives and
 *   completion of directives, module paths and versions (`go_mod_parse`, `go_mod_complete`),
 * - ▶ on `//go:generate` directives (Run 'go generate'), gutter ▶ for tests come from the
 *   shared test runner (`go-test`),
 * - Reformat Code with gofmt / goimports (`go.formatter`),
 * - go vet and golangci-lint problems after saving (`go.vet.onSave`,
 *   `go.golangciLint.onSave`) and on demand.
 */
import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { EditorState, type Extension } from "@codemirror/state";
import type { StringStream } from "@codemirror/language";
import { Play } from "lucide-solid";
import { goApi, type GoProblem } from "@/ipc/go";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { registerEditorExtension } from "@/editor/extensions";
import { registerLanguage } from "@/editor/languages";
import { registerLineMarkerProvider } from "@/editor/lineMarkers";
import { streamLanguage } from "@/editor/tokens";
import { publishDiagnostics } from "@/features/lsp/store";
import { registerFormatProvider } from "@/features/lsp/format";
import { onFileSaved } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { goActive, runGoCommand, showGoBalloon } from "./state";

const fileName = (path: string) => path.replace(/\\/g, "/").split("/").pop() ?? path;
export const isGoPath = (path: string) => path.endsWith(".go");
export const isModPath = (path: string) => ["go.mod", "go.work"].includes(fileName(path));

// ---- go.mod / go.work / go.sum syntax --------------------------------------------------------

const DIRECTIVES = new Set(["module", "go", "toolchain", "godebug", "require", "replace", "exclude", "retract", "tool", "ignore", "use"]);

interface ModState {
  /** Words since the line start (0: directive position). */
  word: number;
  block: boolean;
}

const modParser = {
  name: "go.mod",
  startState: (): ModState => ({ word: 0, block: false }),
  token(stream: StringStream, state: ModState): string | null {
    if (stream.sol()) state.word = 0;
    if (stream.eatSpace()) return null;
    if (stream.match("//")) {
      stream.skipToEnd();
      return "comment";
    }
    if (stream.match("=>")) return "operator";
    const ch = stream.peek();
    if (ch === "(" || ch === ")") {
      stream.next();
      state.block = ch === "(";
      return "bracket";
    }
    if (ch === '"' || ch === "`") {
      stream.next();
      while (!stream.eol() && stream.next() !== ch) {
        /* skip */
      }
      state.word++;
      return "string";
    }
    stream.eatWhile(/[^\s()"`]/);
    const w = stream.current();
    const first = state.word === 0;
    state.word++;
    if (first && DIRECTIVES.has(w)) return "keyword";
    if (/^v\d+\.\d+\.\d+/.test(w) || /^go?\d+\.\d+/.test(w) || /^\d+\.\d+(\.\d+)?$/.test(w)) return "number";
    if (/^h1:/.test(w)) return "atom";
    if (w.startsWith("./") || w.startsWith("../")) return "string";
    return "namespace";
  },
};

registerLanguage({ id: "go-mod", load: () => streamLanguage(modParser) });
registerLanguage({ id: "go-work", load: () => streamLanguage(modParser) });
registerLanguage({ id: "go-sum", load: () => streamLanguage(modParser) });

// ---- go.mod problems and go:generate markers ----------------------------------------------

registerLineMarkerProvider({
  appliesTo: (path) => isModPath(path) || isGoPath(path),
  watch: () => goActive(),
  markers: async (path, text) => {
    if (!goActive()) return [];
    if (isModPath(path)) {
      const parsed = await goApi.modParse(path, text).catch(() => null);
      const diags: EditorDiagnostic[] = (parsed?.problems ?? []).map((p) => ({
        range: { start: { line: p.line, character: p.start }, end: { line: p.line, character: p.end } },
        severity: p.severity,
        message: p.message,
        source: fileName(path),
      }));
      publishDiagnostics(path, diags, "goforge-mod");
      return [];
    }
    if (!text.includes("//go:generate")) return [];
    const directives = await goApi.generateDirectives(text).catch(() => []);
    return directives.map((d) => ({
      line: d.line,
      icon: Play,
      title: `Run 'go generate' (${d.command.split(/\s+/)[0]})`,
      className: "marker-run",
      onClick: () => void runGenerate(path, text, d.line),
    }));
  },
});

/** `go generate` for a file, or for one of its directives. */
export async function runGenerate(path: string, text: string, line?: number): Promise<void> {
  try {
    const cmd = await goApi.generateCommand(path, text, line);
    await runGoCommand(cmd, `go generate ${fileName(path)}`);
  } catch (e) {
    showGoBalloon("go generate", String(e).replace(/^Error: /, ""), 6000);
  }
}

// ---- Completion in go.mod / go.work -----------------------------------------------------------

function completionSource(path: string) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!goActive()) return null;
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    if (!ctx.explicit && !/[\w./-]$/.test(before)) return null;
    const answer = await goApi.modComplete(path, ctx.state.doc.toString(), ctx.pos).catch(() => null);
    if (!answer || ctx.aborted || !answer.items.length) return null;
    // `from` is a column of the line.
    const from = line.from + answer.from;
    return {
      from,
      options: answer.items.map((item, i) => ({
        label: item.label,
        detail: item.detail ?? undefined,
        type: item.kind === "directive" ? "keyword" : item.kind === "module" ? "namespace" : "constant",
        boost: item.kind === "version" ? -i : undefined,
      })),
      filter: answer.items[0]?.kind !== "version",
      validFor: /^[\w./!+-]*$/,
    };
  };
}

registerEditorExtension(({ path }): Extension | null => {
  if (!isModPath(path)) return null;
  const data = [{ autocomplete: completionSource(path) }];
  return EditorState.languageData.of(() => data);
});

// ---- Reformat Code --------------------------------------------------------------------------

registerFormatProvider(async (path, text) => {
  if (!isGoPath(path) || !goActive()) return null;
  const formatter = getSetting<string>("go.formatter", "gofmt");
  if (formatter === "none") return text;
  if (formatter === "lsp") return null;
  try {
    return (await goApi.format(path, text)) ?? null;
  } catch (e) {
    showGoBalloon(formatter, String(e).replace(/^Error: /, ""), 6000);
    return text;
  }
});

// ---- go vet and golangci-lint -----------------------------------------------------------------

const SOURCES = { vet: "go-vet", golangci: "golangci-lint" } as const;
/** Files that got problems from each tool, so a new run clears the old ones. */
const published: Record<keyof typeof SOURCES, Set<string>> = { vet: new Set(), golangci: new Set() };

/** Publish a tool's problems; files of `scope` (a package folder, or everything) without problems are cleared. */
export function publishGoProblems(tool: keyof typeof SOURCES, problems: GoProblem[], scope?: string): void {
  const byFile = new Map<string, GoProblem[]>();
  for (const p of problems) byFile.set(p.path, [...(byFile.get(p.path) ?? []), p]);
  const inScope = (f: string) => scope === undefined || f.replace(/\/[^/]*$/, "") === scope;
  for (const f of published[tool]) if (inScope(f) && !byFile.has(f)) publishDiagnostics(f, [], SOURCES[tool]);
  for (const [file, list] of byFile) {
    publishDiagnostics(
      file,
      list.map((p) => ({
        range: { start: { line: p.line, character: p.column }, end: { line: p.line, character: p.column + 1 } },
        severity: p.severity,
        message: p.message,
        source: p.source,
        code: p.code,
      })),
      SOURCES[tool],
    );
    published[tool].add(file);
  }
}

/** Run go vet or golangci-lint for a file's package (all packages without a path). */
export async function runLint(tool: "vet" | "golangci", path?: string, quiet = false): Promise<GoProblem[] | null> {
  try {
    const problems = await goApi.lint(tool, path);
    publishGoProblems(tool, problems, path ? path.replace(/\/[^/]*$/, "").replace(/^[^/]*$/, "") : undefined);
    if (!quiet) showGoBalloon(tool === "vet" ? "go vet" : "golangci-lint", problems.length ? `${problems.length} problem${problems.length === 1 ? "" : "s"} found` : "No problems found", 4000);
    return problems;
  } catch (e) {
    if (!quiet) showGoBalloon(tool === "vet" ? "go vet" : "golangci-lint", String(e).replace(/^Error: /, ""), 8000);
    return null;
  }
}

if (typeof document !== "undefined") {
  onFileSaved((path) => {
    if (!isGoPath(path) || !goActive()) return;
    if (getSetting<boolean>("go.vet.onSave", true)) void runLint("vet", path, true);
    if (getSetting<boolean>("go.golangciLint.onSave", false)) void runLint("golangci", path, true);
  });
}
