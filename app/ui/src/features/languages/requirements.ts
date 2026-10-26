/**
 * pip requirements files (`requirements*.txt`, `constraints*.txt`): highlighting of package
 * names, extras, version specifiers, environment markers, options (`-r`, `--index-url`),
 * URLs and comments. Inspections, completion and quick fixes come from PyForge
 * (`features/python`).
 */
import type { StreamParser, StringStream } from "@codemirror/language";
import { registerLanguage } from "@/editor/languages";
import { streamLanguage } from "./shared";

interface State {
  /** Where the line is: before the name, after it, in the extras, in the marker. */
  mode: "start" | "spec" | "extras" | "marker" | "option";
}

export const requirementsParser: StreamParser<State> = {
  name: "pip-requirements",
  startState: () => ({ mode: "start" }),
  copyState: (s) => ({ ...s }),
  token(stream: StringStream, state) {
    if (stream.sol()) state.mode = "start";
    if (stream.eatSpace()) return null;
    if (stream.peek() === "#" && (stream.pos === 0 || /\s/.test(stream.string[stream.pos - 1] ?? ""))) {
      stream.skipToEnd();
      return "comment";
    }
    if (stream.match(/^\\$/)) return "escape";
    if (stream.match(/^[a-z][a-z0-9+.-]*:\/\/\S+/i) || stream.match(/^git\+\S+/)) return "url";
    switch (state.mode) {
      case "start":
        if (stream.match(/^--?[A-Za-z][\w-]*/)) {
          state.mode = "option";
          return "option";
        }
        if (stream.match(/^[.\/~][^\s;#]*/)) return "string";
        if (stream.match(/^[A-Za-z0-9][A-Za-z0-9._-]*/)) {
          state.mode = "spec";
          return "def";
        }
        break;
      case "option":
        if (stream.match(/^=/)) return "operator";
        if (stream.match(/^[^\s#]+/)) return "string";
        break;
      case "spec":
        if (stream.match(/^\[/)) {
          state.mode = "extras";
          return "bracket";
        }
        if (stream.match(/^(===|~=|==|!=|<=|>=|<|>)/)) return "operator";
        if (stream.match(/^[0-9][\w.*+!-]*/)) return "number";
        if (stream.match(/^[,()]/)) return "punct";
        if (stream.match(/^@/)) return "operator";
        if (stream.match(/^;/)) {
          state.mode = "marker";
          return "punct";
        }
        if (stream.match(/^--[\w-]+/)) return "option";
        break;
      case "extras":
        if (stream.match(/^\]/)) {
          state.mode = "spec";
          return "bracket";
        }
        if (stream.match(/^,/)) return "punct";
        if (stream.match(/^[\w.-]+/)) return "property";
        break;
      case "marker":
        if (stream.match(/^(and|or|in|not)\b/)) return "keyword";
        if (stream.match(/^(python_version|python_full_version|os_name|sys_platform|platform_\w+|implementation_\w+|extra)\b/)) return "variable";
        if (stream.match(/^("[^"]*"?|'[^']*'?)/)) return "string";
        if (stream.match(/^(===|~=|==|!=|<=|>=|<|>)/)) return "operator";
        if (stream.match(/^[()]/)) return "punct";
        break;
    }
    stream.next();
    return null;
  },
  languageData: { commentTokens: { line: "#" } },
};

registerLanguage({
  id: "pip-requirements",
  load: () => streamLanguage(requirementsParser),
});
