/**
 * Syntax highlighting for code in chat messages. PHP uses the real Lezer parser from the
 * editor; other languages get a light regex tokenizer (strings, comments, numbers,
 * keywords). Output is a list of spans with `tok-*` classes styled in chat.css.
 */
import { classHighlighter, highlightCode } from "@lezer/highlight";
import { phpLanguage } from "@codemirror/lang-php";

export interface Span {
  text: string;
  cls: string;
}

/** Spans per line. */
export type HighlightedLines = Span[][];

const phpProgram = phpLanguage.parser.configure({ top: "Program" });

function lezerHighlight(code: string, parser: typeof phpProgram): HighlightedLines {
  const lines: HighlightedLines = [[]];
  highlightCode(
    code,
    parser.parse(code),
    classHighlighter,
    (text, cls) => lines[lines.length - 1].push({ text, cls }),
    () => lines.push([]),
  );
  return lines;
}

const KEYWORDS = new Set(
  (
    "if else elif fi then for while do done case esac in function return const let var new class extends implements " +
    "import from export default async await try catch finally throw public private protected static use namespace " +
    "true false null undefined echo def lambda pass and or not is local end self this interface type enum match fn mut pub struct impl"
  ).split(" "),
);

const TOKEN_RE =
  /(\/\/[^\n]*|#[^\n]*|\/\*[\s\S]*?\*\/|--[^\n]*)|("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`)|(\b\d+(?:\.\d+)?\b)|(\$\{?\w+\}?)|([A-Za-z_][\w-]*)/g;

function regexHighlight(code: string, lang: string): HighlightedLines {
  const comments = lang === "sql" || lang === "lua" ? /^--/ : /^(\/\/|\/\*|#)/;
  const spans: Span[] = [];
  let last = 0;
  for (const m of code.matchAll(TOKEN_RE)) {
    const i = m.index ?? 0;
    if (i > last) spans.push({ text: code.slice(last, i), cls: "" });
    let cls = "";
    if (m[1]) cls = comments.test(m[1]) ? "tok-comment" : "";
    else if (m[2]) cls = "tok-string";
    else if (m[3]) cls = "tok-number";
    else if (m[4]) cls = "tok-variableName";
    else if (m[5] && KEYWORDS.has(m[5])) cls = "tok-keyword";
    // A `#` that is not a comment for this language (e.g. CSS colors) is re-scanned as text.
    spans.push({ text: m[0], cls });
    last = i + m[0].length;
  }
  if (last < code.length) spans.push({ text: code.slice(last), cls: "" });
  // Split spans on newlines.
  const lines: HighlightedLines = [[]];
  for (const s of spans) {
    const parts = s.text.split("\n");
    parts.forEach((p, k) => {
      if (k > 0) lines.push([]);
      if (p) lines[lines.length - 1].push({ text: p, cls: s.cls });
    });
  }
  return lines;
}

const plain = (code: string): HighlightedLines => code.split("\n").map((l) => (l ? [{ text: l, cls: "" }] : []));

export function highlight(code: string, lang: string): HighlightedLines {
  const l = lang.toLowerCase();
  try {
    if (l === "php" || (!l && code.trimStart().startsWith("<?php"))) {
      return code.trimStart().startsWith("<?php")
        ? lezerHighlight(code, phpLanguage.parser)
        : lezerHighlight(code, phpProgram);
    }
    if (["", "text", "txt", "plain", "diff"].includes(l)) return plain(code);
    return regexHighlight(code, l);
  } catch {
    return plain(code);
  }
}

/** Guess a fence language from a file path. */
export function langOfPath(path: string): string {
  if (path.endsWith(".blade.php")) return "php";
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return ({ php: "php", js: "js", ts: "ts", tsx: "ts", json: "json", sh: "bash", lua: "lua", sql: "sql", css: "css" } as Record<string, string>)[ext] ?? "";
}
