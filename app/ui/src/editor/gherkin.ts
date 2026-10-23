/**
 * Gherkin syntax for Behat `.feature` files: `Feature`, `Background`, `Scenario`,
 * `Scenario Outline` and `Examples` headings, step keywords (`Given`, `When`, `Then`, `And`,
 * `But`, `*`), tags, comments, tables (`|` cells, the header row of examples), doc strings,
 * quoted step arguments, numbers and outline placeholders (`<count>`). Step matching and
 * navigation are computed in Rust (`phpforge-test-frameworks`).
 */
import type { StreamParser, StringStream } from "@codemirror/language";

interface State {
  /** Inside a `"""` / ```` ``` ```` doc string (the fence that closes it). */
  docString: string | null;
  /** Where the line is: its start, after a heading keyword, in a step, in a table row. */
  mode: "start" | "heading" | "step" | "table";
  /** The next table row is a header (right after `Examples:`). */
  headerNext: boolean;
  header: boolean;
}

const HEADINGS = /^(?:Feature|Business Need|Ability|Background|Scenario Outline|Scenario Template|Scenario|Example|Examples|Scenarios|Rule)(?=:)/;
const STEPS = /^(?:Given|When|Then|And|But)(?=\s)|^\*(?=\s|$)/;

export const gherkin: StreamParser<State> = {
  name: "gherkin",
  startState: () => ({ docString: null, mode: "start", headerNext: false, header: false }),
  copyState: (s) => ({ ...s }),
  token(stream: StringStream, state) {
    if (stream.sol()) {
      state.mode = "start";
      state.header = false;
    }
    if (state.docString) {
      if (stream.sol() && stream.match(new RegExp(`^\\s*${state.docString === "```" ? "```" : '"""'}`))) {
        state.docString = null;
        stream.skipToEnd();
        return "special";
      }
      stream.skipToEnd();
      return "string";
    }
    if (state.mode === "start") {
      if (stream.eatSpace()) return null;
      if (stream.match(/^"""|^```/)) {
        state.docString = stream.current();
        stream.skipToEnd();
        return "special";
      }
      if (stream.match(/^#.*/)) return "comment";
      if (stream.peek() === "@") {
        stream.match(/^@\S+/);
        return "meta";
      }
      const heading = stream.match(HEADINGS) as RegExpMatchArray | null;
      if (heading) {
        state.headerNext = /^(Examples|Scenarios)$/.test(heading[0]);
        state.mode = "heading";
        return "keyword";
      }
      if (stream.match(STEPS)) {
        state.mode = "step";
        state.headerNext = false;
        return "keyword";
      }
      if (stream.peek() === "|") {
        state.mode = "table";
        state.header = state.headerNext;
        state.headerNext = false;
      } else {
        // A description line.
        stream.skipToEnd();
        return "comment";
      }
    }
    if (state.mode === "heading") {
      if (stream.eat(":")) return "punct";
      if (stream.match(/^\s*<[^>\n]*>/)) return "variable";
      stream.match(/^[^<]+/) || stream.next();
      return "heading";
    }
    if (state.mode === "table") {
      if (stream.eatSpace()) return null;
      if (stream.eat("|")) return "punct";
      if (stream.match(/^<[^>\n|]*>/)) return "variable";
      stream.match(/^[^|]+?(?=\s*\||\s*$)/) || stream.next();
      return state.header ? "property" : /^-?\d+(\.\d+)?$/.test(stream.current().trim()) ? "number" : "string";
    }
    // A step.
    if (stream.eatSpace()) return null;
    if (stream.match(/^"(?:[^"\\]|\\.)*"?/) || stream.match(/^'(?:[^'\\]|\\.)*'?/)) return "string";
    if (stream.match(/^<[^>\n]*>/)) return "variable";
    if (stream.match(/^-?\d+(?:\.\d+)?\b/)) return "number";
    stream.match(/^[^\s"'<\d]+/) || stream.next();
    return null;
  },
  languageData: { commentTokens: { line: "#" } },
};
