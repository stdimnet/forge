/**
 * Syntax highlighting of `.http` request files (PHPStorm's HTTP Client format): `###`
 * separators, comments and `# @name` tags, `@variable = value` lines, request lines, headers,
 * `{{variables}}` and JSON-like bodies. Parsing for running requests happens in Rust
 * (`forge_scratches::http`); this tokenizer only colors text per keystroke.
 */
import type { StreamParser, StringStream } from "@codemirror/language";
import { registerLanguage } from "@/editor/languages";
import { streamLanguage } from "@/editor/tokens";

interface State {
  section: "pre" | "headers" | "body";
  /** Where on a request or header line we are. */
  part: "start" | "url" | "value";
}

const METHOD = /^(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|TRACE|CONNECT)\b/i;

function variable(stream: StringStream): boolean {
  return !!stream.match(/^\{\{[^}\n]*\}\}/);
}

export const httpParser: StreamParser<State> = {
  name: "http",
  startState: () => ({ section: "pre", part: "start" }),
  copyState: (s) => ({ ...s }),
  blankLine(state) {
    if (state.section === "headers") state.section = "body";
  },
  token(stream, state) {
    if (stream.sol()) {
      state.part = "start";
      if (stream.match(/^\s*###/)) {
        stream.skipToEnd();
        state.section = "pre";
        return "heading";
      }
    }
    if (state.section === "pre") {
      if (stream.sol() && stream.eatSpace()) return null;
      if (state.part === "start") {
        if (stream.match(/^(#|\/\/)/)) {
          if (stream.match(/^\s*@\w+/)) return "meta";
          stream.skipToEnd();
          return "comment";
        }
        if (stream.match(/^@[\w.-]+/)) {
          state.part = "value";
          return "def";
        }
        if (stream.match(METHOD)) {
          state.part = "url";
          if (stream.eol()) state.section = "headers";
          return "keyword";
        }
        state.part = "url";
      }
      if (state.part === "value") {
        if (stream.eatSpace()) return null;
        if (stream.eat("=")) return "operator";
        if (variable(stream)) return "variable";
        stream.match(/^[^{]+|^\{/);
        return "string";
      }
      // Request line: URL, then the HTTP version.
      let token: string | null = null;
      if (stream.eatSpace()) token = null;
      else if (variable(stream)) token = "variable";
      else if (stream.match(/^HTTP\/[\d.]+/)) token = "meta";
      else {
        stream.match(/^[^\s{]+|^\{/);
        token = "url";
      }
      if (stream.eol()) state.section = "headers";
      return token;
    }
    if (state.section === "headers") {
      if (state.part === "start") {
        if (stream.match(/^\s+[?&]/)) {
          stream.match(/^[^\s{]*/);
          return "url";
        }
        if (stream.match(/^(#|\/\/)/)) {
          stream.skipToEnd();
          return "comment";
        }
        if (stream.match(/^[^:\s]+(?=\s*:)/)) {
          state.part = "value";
          return "property";
        }
        state.part = "value";
      }
      if (stream.eatSpace()) return null;
      if (stream.eat(":")) return "punct";
      if (variable(stream)) return "variable";
      stream.match(/^[^{]+|^\{/);
      return "string";
    }
    // Body.
    if (stream.sol() && stream.match(/^(> \{%|%\}|<> |>>!? |> )/)) {
      stream.skipToEnd();
      return "meta";
    }
    if (stream.eatSpace()) return null;
    if (variable(stream)) return "variable";
    if (stream.match(/^"(?:[^"\\]|\\.)*"?(?=\s*:)/)) return "property";
    if (stream.match(/^"(?:[^"\\]|\\.)*"?/)) return "string";
    if (stream.match(/^-?\d+(\.\d+)?([eE][+-]?\d+)?/)) return "number";
    if (stream.match(/^(true|false|null)\b/)) return "atom";
    if (stream.match(/^[{}[\]]/)) return "bracket";
    if (stream.match(/^[,:]/)) return "punct";
    stream.next();
    return null;
  },
  languageData: { commentTokens: { line: "#" } },
};

registerLanguage({ id: "http", load: () => streamLanguage(httpParser) });
