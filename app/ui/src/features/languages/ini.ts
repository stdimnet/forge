/**
 * `.ini` (php.ini, .editorconfig, .gitconfig, systemd units, …) and `.conf` files
 * (key/value configs such as sysctl.conf, nginx-style blocks): sections, keys, values,
 * interpolation and comments, section folding, and checks for broken sections and
 * duplicated keys.
 */
import { foldService, type StreamParser, type StringStream } from "@codemirror/language";
import { registerEditorExtension } from "@/editor/extensions";
import { languageIdFor, registerLanguage } from "@/editor/languages";
import { streamLanguage, syntaxDiagnostics, type SyntaxProblem } from "./shared";

const REFERENCE = /^\$\{[^}\n]*\}?|^\$[A-Za-z_]\w*|^%\([^)\n]*\)s?/;

type Flavor = "ini" | "conf";

interface State {
  /** Where on the line: before the key, after it (separator expected), in the value. */
  mode: "key" | "sep" | "value";
}

function valueToken(stream: StringStream, flavor: Flavor): string | null {
  if (stream.eatSpace()) return null;
  const prev = stream.string.charAt(stream.pos - 1) || " ";
  const comment = flavor === "ini" ? /[;#]/ : /#/;
  if (comment.test(stream.peek()!) && /\s/.test(prev)) {
    stream.skipToEnd();
    return "comment";
  }
  if (stream.match(REFERENCE)) return "variable";
  const quote = stream.peek();
  if (quote === '"' || quote === "'") {
    stream.next();
    let escaped = false;
    while (!stream.eol()) {
      const ch = stream.next();
      if (ch === "\\" && !escaped) escaped = true;
      else if (ch === quote && !escaped) break;
      else escaped = false;
    }
    return "string";
  }
  if (flavor === "conf" && stream.match(/^[{};]/)) return stream.current() === ";" ? "punct" : "bracket";
  if (stream.match(/^[+-]?\d+(?:\.\d+)?(?:[kKmMgGtT][bB]?|[smhdw]|ms)?(?![\w.])/)) return "number";
  if (stream.match(/^(?:true|false|yes|no|on|off|null|none)(?![\w.])/i)) return "atom";
  if (stream.match(/^[\w.-]+:\/\/[^\s#;]*/)) return "url";
  while (!stream.eol()) {
    const ch = stream.peek()!;
    if (ch === "$" && REFERENCE.test(stream.string.slice(stream.pos))) break;
    if (comment.test(ch) && /\s/.test(stream.string.charAt(stream.pos - 1))) break;
    if (flavor === "conf" && /[{};]/.test(ch)) break;
    if (ch === '"' || ch === "'") break;
    stream.next();
  }
  if (stream.pos === stream.start) stream.next();
  return "string";
}

function parser(flavor: Flavor): StreamParser<State> {
  return {
    name: flavor,
    startState: () => ({ mode: "key" }),
    copyState: (s) => ({ ...s }),
    token(stream, state) {
      if (stream.sol()) state.mode = "key";
      if (state.mode === "value") return valueToken(stream, flavor);
      if (stream.eatSpace()) return null;
      const comment = flavor === "ini" ? /^[;#]/ : /^#/;
      if (state.mode === "key") {
        if (stream.match(comment)) {
          stream.skipToEnd();
          return "comment";
        }
        if (stream.match(/^\[[^\]\n]*\]?/)) {
          return stream.current().endsWith("]") ? "heading" : "invalid";
        }
        if (flavor === "conf" && stream.match(/^[{}]/)) return "bracket";
        const key = stream.match(flavor === "ini" ? /^[^=:\s;#][^=:;#]*?(?=\s*(?:[=:]|$))/ : /^[^\s=:#{};][^\s=:{};]*/);
        if (key) {
          state.mode = "sep";
          return "def";
        }
        stream.skipToEnd();
        return "invalid";
      }
      // Separator.
      if (stream.match(/^[=:]/)) {
        state.mode = "value";
        return "operator";
      }
      state.mode = "value";
      return valueToken(stream, flavor);
    },
    languageData: { commentTokens: { line: flavor === "ini" ? ";" : "#" } },
  };
}

export const iniParser = parser("ini");
export const confParser = parser("conf");

/** Fold from a `[section]` header to the line before the next one. */
function sectionFolding() {
  return foldService.of((state, lineStart) => {
    const line = state.doc.lineAt(lineStart);
    if (!/^\s*\[[^\]]*\]/.test(line.text)) return null;
    let end = line.to;
    for (let n = line.number + 1; n <= state.doc.lines; n++) {
      const next = state.doc.line(n);
      if (/^\s*\[[^\]]*\]/.test(next.text)) break;
      if (next.text.trim()) end = next.to;
    }
    return end > line.to ? { from: line.to, to: end } : null;
  });
}

/** Unclosed section headers and keys repeated within a section. */
export function iniProblems(text: string): SyntaxProblem[] {
  const problems: SyntaxProblem[] = [];
  let section = "";
  let keys = new Map<string, number>();
  let offset = 0;
  const lines = text.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i].replace(/\r$/, "");
    const start = offset;
    offset += lines[i].length + 1;
    const trimmed = raw.trim();
    if (!trimmed || /^[;#]/.test(trimmed)) continue;
    const lead = raw.length - raw.trimStart().length;
    if (trimmed.startsWith("[")) {
      if (!/^\[[^\]]*\]\s*(?:[;#].*)?$/.test(trimmed)) {
        problems.push({ from: start + lead, to: start + raw.length, message: "Section header is missing its closing `]`" });
      }
      section = trimmed;
      keys = new Map();
      continue;
    }
    const m = /^([^=:;#]+?)\s*[=:]/.exec(trimmed);
    if (!m || m[1].endsWith("[]") || /\[[^\]]+\]$/.test(m[1])) continue;
    const first = keys.get(m[1]);
    if (first !== undefined) {
      problems.push({
        from: start + lead,
        to: start + lead + m[1].length,
        message: `\`${m[1]}\` is already set${section ? ` in ${section}` : ""} on line ${first}`,
        severity: "weak",
      });
    } else keys.set(m[1], i + 1);
  }
  return problems;
}

export const iniSupport = () => [streamLanguage(iniParser), sectionFolding()];
export const confSupport = () => [streamLanguage(confParser), sectionFolding()];

registerLanguage({
  id: "ini",
  load: iniSupport,
});

registerLanguage({
  id: "conf",
  load: confSupport,
});

registerEditorExtension(({ path }) =>
  languageIdFor(path) === "ini" ? syntaxDiagnostics(path, { source: "INI" }, (s) => iniProblems(s.doc.toString())) : null,
);
