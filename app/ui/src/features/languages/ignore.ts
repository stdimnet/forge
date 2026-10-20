/**
 * Ignore files (`.gitignore`, `.dockerignore`, `.npmignore`, …): gitignore pattern syntax
 * with comments, negation, wildcards, character classes and escapes, Symfony Flex section
 * folding, and a hint for duplicated patterns.
 */
import type { StreamParser } from "@codemirror/language";
import { registerEditorExtension } from "@/editor/extensions";
import { languageIdFor, registerLanguage } from "@/editor/languages";
import { markerFolding, streamLanguage, syntaxDiagnostics, type SyntaxProblem } from "./shared";

export const ignoreParser: StreamParser<Record<string, never>> = {
  name: "ignore",
  startState: () => ({}),
  token(stream) {
    if (stream.sol()) {
      if (stream.match(/^#.*/)) return stream.string.startsWith("###") ? "meta" : "comment";
      if (stream.match(/^\s*$/)) return null;
      if (stream.match(/^!/)) return "keyword";
    }
    if (stream.match(/^\\./)) return "escape";
    if (stream.match("**")) return "regexp";
    if (stream.match(/^[*?]/)) return "regexp";
    if (stream.match(/^\[(?:\\.|[^\]\\])*\]?/)) return "regexp";
    if (stream.eat("/")) return "punct";
    if (stream.match(/^\s+$/)) return null;
    stream.eatWhile((ch) => !/[*?[\\/]/.test(ch));
    return "string";
  },
  languageData: { commentTokens: { line: "#" } },
};

/** Patterns written twice, which is harmless but usually a leftover. */
export function ignoreProblems(text: string): SyntaxProblem[] {
  const problems: SyntaxProblem[] = [];
  const seen = new Map<string, number>();
  let offset = 0;
  const lines = text.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i].replace(/\r$/, "");
    const start = offset;
    offset += lines[i].length + 1;
    const pattern = raw.replace(/(?<!\\)\s+$/, "");
    if (!pattern.trim() || pattern.startsWith("#")) continue;
    const first = seen.get(pattern);
    if (first !== undefined) {
      problems.push({ from: start, to: start + pattern.length, message: `Duplicate pattern, first used on line ${first}`, severity: "weak" });
    } else seen.set(pattern, i + 1);
  }
  return problems;
}

export function ignoreSupport() {
  return [streamLanguage(ignoreParser), markerFolding()];
}

registerLanguage({
  id: "ignore",
  label: "Ignore File",
  extensions: ["gitignore", "dockerignore", "npmignore", "ignore"],
  fileNames: [".*ignore", ".gitignore_global", ".git/info/exclude"],
  indent: "  ",
  load: ignoreSupport,
});

registerEditorExtension(({ path }) =>
  languageIdFor(path) === "ignore"
    ? syntaxDiagnostics(path, { source: "Ignore file" }, (s) => ignoreProblems(s.doc.toString()))
    : null,
);
