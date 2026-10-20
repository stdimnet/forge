import type { StreamParser } from "@codemirror/language";

interface DotenvState {
  /** Where the next token is on the current line. */
  part: "key" | "value";
  quote: string | null;
}

const LITERAL = /^(true|false|null|empty|\d+(\.\d+)?)\s*(#.*)?$/i;

/**
 * Highlighting for `.env` files the way PHPStorm's Dotenv plugin shows them: comments,
 * keys, values, quoted strings, `${VAR}` references and literal values.
 */
export const dotenv: StreamParser<DotenvState> = {
  name: "dotenv",
  startState: () => ({ part: "key", quote: null }),
  token(stream, state) {
    if (stream.sol() && !state.quote) state.part = "key";

    // Inside a quoted value (may span lines for double quotes).
    if (state.quote) {
      if (state.quote === '"' && stream.match(/^\$\{[^}]*\}/)) return "variableName.special";
      while (!stream.eol()) {
        const ch = stream.next();
        if (ch === "\\") stream.next();
        else if (ch === state.quote) {
          state.quote = null;
          return "string";
        } else if (state.quote === '"' && ch === "$" && stream.peek() === "{") {
          stream.backUp(1);
          return "string";
        }
      }
      return "string";
    }

    if (stream.eatSpace()) return null;
    if (stream.peek() === "#") {
      stream.skipToEnd();
      return "comment";
    }

    if (state.part === "key") {
      if (stream.match(/^export\b/)) return "keyword";
      if (stream.match(/^[A-Za-z_][\w.-]*/)) return "propertyName.definition";
      if (stream.eat("=")) {
        state.part = "value";
        return "operator";
      }
      stream.next();
      return "invalid";
    }

    // Value part.
    const quote = stream.peek();
    if (quote === '"' || quote === "'") {
      stream.next();
      state.quote = quote;
      return "string";
    }
    if (stream.match(/^\$\{[^}]*\}/) || stream.match(/^\$[A-Za-z_]\w*/)) return "variableName.special";
    if (stream.string.slice(stream.pos).match(LITERAL) && stream.match(/^[^\s#]+/)) return "atom";
    if (stream.match(/^[^\s#$]+/)) return "string";
    stream.next();
    return "string";
  },
};
