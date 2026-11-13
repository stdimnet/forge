/**
 * Dotenv syntax for `.env` files (dotenv, Symfony, Laravel, docker): highlighting of keys,
 * quoted and unquoted values and `${VAR}` interpolation, and a parser that lists the variables
 * and the problems that make dotenv ignore or reject a line.
 */
import type { StreamParser, StringStream } from "@codemirror/language";

export interface EnvProblem {
  from: number;
  to: number;
  message: string;
  severity?: "error" | "warning" | "weak" | "info";
}

interface State {
  /** Where the line is: before the key, in the value, or inside a quoted value. */
  mode: "start" | "value" | "double" | "single";
}

const INTERPOLATION = /^\$(?:\{[^}\n]*\}?|\([^)\n]*\)?|[A-Za-z_]\w*)/;

export const dotenv: StreamParser<State> = {
  name: "dotenv",
  startState: () => ({ mode: "start" }),
  copyState: (s) => ({ ...s }),
  token(stream: StringStream, state) {
    if (state.mode === "double" || state.mode === "single") {
      const quote = state.mode === "double" ? '"' : "'";
      if (state.mode === "double") {
        if (stream.match(INTERPOLATION)) return "variable";
        if (stream.match(/^\\./)) return "escape";
      }
      while (!stream.eol()) {
        if (stream.peek() === quote) {
          stream.next();
          state.mode = "value";
          return "string";
        }
        if (state.mode === "double" && (stream.peek() === "\\" || stream.peek() === "$")) {
          if (stream.pos > stream.start) return "string";
          stream.next();
          return "string";
        }
        stream.next();
      }
      return "string";
    }
    if (stream.sol()) state.mode = "start";
    if (state.mode === "start") {
      if (stream.eatSpace()) return null;
      if (stream.match(/^#.*/)) return stream.string.trimStart().startsWith("###") ? "meta" : "comment";
      if (stream.match(/^export(?=\s+[A-Za-z_])/)) return "keyword";
      if (stream.match(/^[A-Za-z_][\w.-]*/)) return "def";
      if (stream.eat("=")) {
        state.mode = "value";
        return "operator";
      }
      stream.skipToEnd();
      return "invalid";
    }
    // Value.
    if (stream.eatSpace()) return null;
    if (stream.peek() === "#" && /\s/.test(stream.string.charAt(stream.pos - 1) || " ")) {
      stream.skipToEnd();
      return "comment";
    }
    if (stream.eat('"')) {
      state.mode = "double";
      return "string";
    }
    if (stream.eat("'")) {
      state.mode = "single";
      return "string";
    }
    if (stream.match(INTERPOLATION)) return "variable";
    if (stream.match(/^\\./)) return "escape";
    const start = stream.pos;
    while (!stream.eol()) {
      const ch = stream.peek()!;
      if (ch === "$" && INTERPOLATION.test(stream.string.slice(stream.pos))) break;
      if (ch === "\\") break;
      if (ch === "#" && /\s/.test(stream.string.charAt(stream.pos - 1))) break;
      stream.next();
    }
    const text = stream.string.slice(start, stream.pos).trim();
    const wholeValue = /=\s*$/.test(stream.string.slice(0, start)) && /^\s*(?:#.*)?$/.test(stream.string.slice(stream.pos));
    if (wholeValue) {
      if (/^(?:true|false|null|yes|no|on|off)$/i.test(text)) return "atom";
      if (/^-?\d+(?:\.\d+)?$/.test(text)) return "number";
    }
    return "string";
  },
  languageData: { commentTokens: { line: "#" } },
};

export interface EnvEntry {
  key: string;
  /** Offsets of the key. */
  from: number;
  to: number;
  value: string;
  line: number;
}

export interface EnvParse {
  entries: EnvEntry[];
  problems: EnvProblem[];
}

/** Variables and problems of a dotenv file. */
export function parseEnv(text: string): EnvParse {
  const entries: EnvEntry[] = [];
  const problems: EnvProblem[] = [];
  const lines = text.split("\n");
  const starts: number[] = [];
  let acc = 0;
  for (const l of lines) {
    starts.push(acc);
    acc += l.length + 1;
  }
  const lineOf = (offset: number) => {
    let lo = 0;
    let hi = starts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (starts[mid] <= offset) lo = mid;
      else hi = mid - 1;
    }
    return lo;
  };
  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i].replace(/\r$/, "");
    if (/^\s*(#.*)?$/.test(raw)) continue;
    const start = starts[i];
    const m = /^(\s*)(?:export\s+)?([A-Za-z_][\w.-]*)([ \t]*=)?(.*)$/.exec(raw);
    if (!m) {
      problems.push({ from: start, to: start + raw.length, message: "Not a valid `KEY=value` line" });
      continue;
    }
    const key = m[2];
    const from = start + raw.indexOf(key, m[1].length);
    const to = from + key.length;
    if (!m[3]) {
      problems.push({ from, to, message: `\`${key}\` has no \`=\`; dotenv declares it empty or ignores the line`, severity: "warning" });
      entries.push({ key, from, to, value: "", line: i + 1 });
      continue;
    }
    const rest = m[4];
    const valueFrom = start + raw.length - rest.length + (rest.length - rest.trimStart().length);
    const quote = rest.trimStart()[0];
    if (quote === '"' || quote === "'") {
      let end = -1;
      for (let k = valueFrom + 1; k < text.length; k++) {
        if (quote === '"' && text[k] === "\\") k++;
        else if (text[k] === quote) {
          end = k;
          break;
        }
      }
      if (end < 0) {
        problems.push({ from, to: start + raw.length, message: `Unterminated ${quote} quote` });
        entries.push({ key, from, to, value: text.slice(valueFrom + 1), line: i + 1 });
        break;
      }
      entries.push({ key, from, to, value: text.slice(valueFrom + 1, end), line: i + 1 });
      const j = lineOf(end);
      const after = lines[j].slice(end - starts[j] + 1).replace(/\r$/, "");
      if (after.trim() && !after.trimStart().startsWith("#")) {
        const at = end + 1 + (after.length - after.trimStart().length);
        problems.push({ from: at, to: end + 1 + after.trimEnd().length, message: "Unexpected text after the closing quote" });
      }
      i = j;
      continue;
    }
    const value = rest.replace(/(?:^|\s)#.*$/, "").trim();
    entries.push({ key, from, to, value, line: i + 1 });
    if (/\s/.test(value) && !/\$\(/.test(value)) {
      problems.push({
        from: valueFrom,
        to: valueFrom + value.length,
        message: "Values with spaces must be quoted (Symfony's dotenv rejects them)",
        severity: "warning",
      });
    }
  }
  const seen = new Map<string, EnvEntry>();
  for (const e of entries) {
    const first = seen.get(e.key);
    if (first) problems.push({ from: e.from, to: e.to, message: `\`${e.key}\` is already defined on line ${first.line}`, severity: "weak" });
    else seen.set(e.key, e);
  }
  return { entries, problems };
}
