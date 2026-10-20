/**
 * Postfix configuration: `main.cf` (and the `*.cf` lookup table definitions) and
 * `master.cf`. Highlighting of parameters, `$references`, lookup table types and access
 * restrictions; completion and hover documentation for the parameters; and checks for
 * malformed and overridden lines.
 */
import type { StreamParser, StringStream } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { registerEditorExtension } from "@/editor/extensions";
import { languageIdFor, registerLanguage } from "@/editor/languages";
import { docDom, hoverDocs, indentFolding, streamLanguage, syntaxDiagnostics, type SyntaxProblem } from "./shared";
import {
  MASTER_COMMANDS,
  MASTER_TYPES,
  POSTFIX_LOOKUP_TYPES,
  POSTFIX_PARAMS,
  POSTFIX_PARAM_MAP,
  POSTFIX_RESTRICTIONS,
  type PostfixParam,
} from "./postfixData";

const LOOKUP_TYPES = new Set(POSTFIX_LOOKUP_TYPES);
const REFERENCE = /^\$\{[^}\n]*\}?|^\$[A-Za-z_][\w]*/;
const NAME = /^[A-Za-z0-9_][A-Za-z0-9_.-]*/;

function valueToken(stream: StringStream): string | null {
  if (stream.eatSpace()) return null;
  if (stream.match(REFERENCE)) return "variable";
  if (stream.eat(",")) return "punct";
  if (stream.match(/^[{}]/)) return "bracket";
  if (stream.match(/^\[[^\]\s]*\](?::\d+)?/)) return "special";
  if (stream.match(/^\/[^\s,]*/)) return "string";
  if (stream.match(/^[a-z]+(?=:)/, false)) {
    const word = (stream.match(/^[a-z]+/, false) as RegExpMatchArray)[0];
    if (LOOKUP_TYPES.has(word)) {
      stream.match(word);
      return "type";
    }
  }
  if (stream.eat(":")) return "punct";
  if (stream.match(/^\d+(?:\.\d+){0,3}(?:\/\d+)?[smhdw]?(?![\w.-])/)) return "number";
  const word = stream.match(/^[A-Za-z_][\w.-]*/) as RegExpMatchArray | null;
  if (word) {
    if (word[0] === "yes" || word[0] === "no") return "atom";
    if (word[0] in POSTFIX_RESTRICTIONS) return "keyword";
    return null;
  }
  stream.next();
  return null;
}

interface MainState {
  /** A logical line is open, so a line starting with whitespace continues it. */
  logical: boolean;
  mode: "name" | "eq" | "value";
}

export const postfixMainParser: StreamParser<MainState> = {
  name: "postfix",
  startState: () => ({ logical: false, mode: "name" }),
  copyState: (s) => ({ ...s }),
  blankLine: () => undefined,
  token(stream, state) {
    if (stream.sol()) {
      const line = stream.string;
      if (!line.trim()) {
        stream.skipToEnd();
        return null;
      }
      if (line.trimStart().startsWith("#")) {
        stream.skipToEnd();
        return "comment";
      }
      if (/^\s/.test(line)) state.mode = state.logical ? "value" : "name";
      else {
        state.mode = "name";
        state.logical = true;
      }
    }
    if (state.mode === "name") {
      if (stream.eatSpace()) return null;
      if (stream.match(NAME)) {
        state.mode = "eq";
        return "def";
      }
      stream.skipToEnd();
      return "invalid";
    }
    if (state.mode === "eq") {
      if (stream.eatSpace()) return null;
      if (stream.eat("=")) {
        state.mode = "value";
        return "operator";
      }
      stream.skipToEnd();
      return "invalid";
    }
    return valueToken(stream);
  },
  languageData: { commentTokens: { line: "#" } },
};

interface MasterState {
  /** Column of the next token on a service line: 0 service … 7 command, 8+ arguments. */
  col: number;
  /** Next word follows `-o`. */
  option: boolean;
  /** Inside the value part of `-o name=value`. */
  value: boolean;
}

export const postfixMasterParser: StreamParser<MasterState> = {
  name: "postfix-master",
  startState: () => ({ col: 0, option: false, value: false }),
  copyState: (s) => ({ ...s }),
  token(stream, state) {
    if (stream.sol()) {
      const line = stream.string;
      if (!line.trim()) {
        stream.skipToEnd();
        return null;
      }
      if (line.trimStart().startsWith("#")) {
        stream.skipToEnd();
        return "comment";
      }
      if (!/^\s/.test(line)) {
        state.col = 0;
        state.option = false;
      }
      state.value = false;
    }
    if (stream.eatSpace()) {
      state.value = false;
      return null;
    }
    if (state.col >= 8 || (state.col === 7 && false)) return argToken(stream, state);
    const col = state.col++;
    switch (col) {
      case 0:
        stream.match(/^[^\s]+/);
        return "def";
      case 1: {
        const w = (stream.match(/^[^\s]+/) as RegExpMatchArray)[0];
        return MASTER_TYPES.includes(w) ? "keyword" : "invalid";
      }
      case 2:
      case 3:
      case 4: {
        const w = (stream.match(/^[^\s]+/) as RegExpMatchArray)[0];
        return /^[yn-]$/.test(w) ? "atom" : "invalid";
      }
      case 5:
      case 6: {
        const w = (stream.match(/^[^\s]+/) as RegExpMatchArray)[0];
        if (w === "-") return "atom";
        return /^\d+\??$/.test(w) ? "number" : "invalid";
      }
      default:
        stream.match(/^[^\s]+/);
        return "builtin";
    }
  },
  languageData: { commentTokens: { line: "#" } },
};

function argToken(stream: StringStream, state: MasterState): string | null {
  if (state.option) {
    if (state.value) return valueToken(stream) ?? null;
    if (stream.match(/^\{/)) return "bracket";
    if (stream.match(/^\}/)) {
      state.option = false;
      return "bracket";
    }
    if (stream.match(/^[A-Za-z0-9_][A-Za-z0-9_.-]*/)) return "def";
    if (stream.eat("=")) {
      state.value = true;
      return "operator";
    }
    stream.next();
    return null;
  }
  if (stream.match(/^-o(?=\s|$)/)) {
    state.option = true;
    return "option";
  }
  if (stream.match(/^-[A-Za-z0-9]+/)) return "option";
  if (stream.match(REFERENCE)) return "variable";
  stream.match(/^[^\s]+/);
  return null;
}

/** Start of the logical line (walking up over continuation, comment and blank lines). */
function logicalStart(state: EditorState, lineNo: number): number {
  for (let n = lineNo; n >= 1; n--) {
    const text = state.doc.line(n).text;
    if (!text.trim() || text.trimStart().startsWith("#")) continue;
    if (!/^\s/.test(text)) return n;
  }
  return lineNo;
}

const paramCompletion = (p: PostfixParam, apply?: string): Completion => ({
  label: p.name,
  type: "property",
  detail: p.def === "empty" ? "" : p.def,
  info: () => docDom({ title: `${p.name} = ${p.def}`, doc: p.doc }),
  apply,
});

const restrictionCompletions: Completion[] = Object.entries(POSTFIX_RESTRICTIONS).map(([label, doc]) => ({
  label,
  type: "keyword",
  info: () => docDom({ title: label, doc }),
}));

/** Completions for the value of `param`. */
function valueCompletions(param: string | undefined): Completion[] {
  const out: Completion[] = [];
  const known = param ? POSTFIX_PARAM_MAP.get(param) : undefined;
  for (const v of known?.values ?? []) out.push({ label: v, type: "constant", boost: 2 });
  if (param && /(?:restrictions|restriction_classes)$/.test(param)) out.push(...restrictionCompletions);
  if (param && /(?:_maps?|_database|_checks|_domains|_list)$/.test(param)) {
    for (const t of POSTFIX_LOOKUP_TYPES) out.push({ label: `${t}:`, type: "namespace", detail: "lookup table" });
  }
  return out;
}

function mainCompletions(ctx: CompletionContext): CompletionResult | null {
  const ref = ctx.matchBefore(/\$\{?[\w]*/);
  if (ref) {
    const braces = ref.text.startsWith("${");
    return {
      from: ref.from + (braces ? 2 : 1),
      options: POSTFIX_PARAMS.map((p) => paramCompletion(p, braces ? `${p.name}}` : p.name)),
      validFor: /^\w*$/,
    };
  }
  const line = ctx.state.doc.lineAt(ctx.pos);
  const before = line.text.slice(0, ctx.pos - line.from);
  if (before.trimStart().startsWith("#")) return null;
  const start = logicalStart(ctx.state, line.number);
  const head = ctx.state.doc.line(start).text;
  const name = /^([A-Za-z0-9_][A-Za-z0-9_.-]*)\s*=/.exec(head)?.[1];
  const onNameLine = start === line.number;
  if (onNameLine && !before.includes("=")) {
    if (/^\s/.test(before)) return null;
    const word = ctx.matchBefore(/[A-Za-z0-9_.-]*/)!;
    if (word.from === word.to && !ctx.explicit) return null;
    return {
      from: word.from,
      options: POSTFIX_PARAMS.map((p) => paramCompletion(p, `${p.name} = `)),
      validFor: /^[\w.-]*$/,
    };
  }
  const word = ctx.matchBefore(/[\w.:-]*/)!;
  if (word.from === word.to && !ctx.explicit) return null;
  const options = valueCompletions(name);
  return options.length ? { from: word.from, options, validFor: /^[\w.:-]*$/ } : null;
}

function masterCompletions(ctx: CompletionContext): CompletionResult | null {
  const line = ctx.state.doc.lineAt(ctx.pos);
  const before = line.text.slice(0, ctx.pos - line.from);
  if (before.trimStart().startsWith("#")) return null;
  const ref = ctx.matchBefore(/\$\{?[\w]*/);
  if (ref) {
    const braces = ref.text.startsWith("${");
    return {
      from: ref.from + (braces ? 2 : 1),
      options: POSTFIX_PARAMS.map((p) => paramCompletion(p, braces ? `${p.name}}` : p.name)),
      validFor: /^\w*$/,
    };
  }
  // Value of `-o name=`.
  const optValue = /-o\s+\{?\s*([A-Za-z0-9_.-]+)\s*=\s*([\w.:-]*)$/.exec(before);
  if (optValue) {
    const options = valueCompletions(optValue[1]);
    return options.length ? { from: ctx.pos - optValue[2].length, options, validFor: /^[\w.:-]*$/ } : null;
  }
  const optName = /-o\s+\{?\s*([A-Za-z0-9_.-]*)$/.exec(before);
  if (optName) {
    return {
      from: ctx.pos - optName[1].length,
      options: POSTFIX_PARAMS.map((p) => paramCompletion(p, `${p.name}=`)),
      validFor: /^[\w.-]*$/,
    };
  }
  const continuation = /^\s/.test(line.text);
  if (continuation) {
    return /(?:^|\s)-?[\w]*$/.test(before)
      ? { from: ctx.pos - (/[\w-]*$/.exec(before)?.[0].length ?? 0), options: [{ label: "-o", type: "keyword", detail: "main.cf override" }], validFor: /^-?\w*$/ }
      : null;
  }
  const tokens = before.trim() ? before.split(/\s+/) : [""];
  const col = /\s$/.test(before) ? tokens.filter(Boolean).length : tokens.length - 1;
  const word = ctx.matchBefore(/[\w.-]*/)!;
  const from = word.from;
  if (col === 1) return { from, options: MASTER_TYPES.map((label) => ({ label, type: "keyword" })), validFor: /^[\w-]*$/ };
  if (col >= 2 && col <= 4) {
    return { from, options: ["y", "n", "-"].map((label) => ({ label, type: "constant" })), validFor: /^[yn-]?$/ };
  }
  if (col === 7) {
    return {
      from,
      options: MASTER_COMMANDS.map((c) => ({ label: c.name, type: "function", info: () => docDom({ title: c.name, doc: c.doc }) })),
      validFor: /^[\w-]*$/,
    };
  }
  if (col >= 8) return { from, options: [{ label: "-o", type: "keyword", detail: "main.cf override" }], validFor: /^-?\w*$/ };
  return null;
}

function paramHover(state: EditorState, pos: number) {
  const line = state.doc.lineAt(pos);
  const text = line.text;
  const col = pos - line.from;
  for (const m of text.matchAll(/\$\{?([A-Za-z_]\w*)|(?:^|(?<=\s)-o\s+\{?\s*)([A-Za-z0-9_][A-Za-z0-9_.-]*)(?=\s*=)|^([A-Za-z0-9_][A-Za-z0-9_.-]*)(?=\s*=)/g) as Iterable<RegExpMatchArray>) {
    const name = m[1] ?? m[2] ?? m[3];
    const nameStart = m.index! + m[0].lastIndexOf(name);
    if (col < nameStart || col > nameStart + name.length) continue;
    const p = POSTFIX_PARAM_MAP.get(name);
    if (!p) return null;
    const from = line.from + nameStart;
    return { from, to: from + name.length, dom: () => docDom({ title: `${p.name} = ${p.def}`, doc: p.doc }) };
  }
  const word = state.wordAt(pos);
  if (word) {
    const w = state.sliceDoc(word.from, word.to);
    if (w in POSTFIX_RESTRICTIONS) return { from: word.from, to: word.to, dom: () => docDom({ title: w, doc: POSTFIX_RESTRICTIONS[w] }) };
  }
  return null;
}

function masterHover(state: EditorState, pos: number) {
  const line = state.doc.lineAt(pos);
  if (!/^\s/.test(line.text)) {
    const m = /^(\S+)(\s+\S+){6}\s+(\S+)/.exec(line.text);
    if (m) {
      const from = line.from + line.text.lastIndexOf(m[3], line.text.length);
      const idx = line.text.indexOf(m[3], m[0].length - m[3].length);
      const cmd = MASTER_COMMANDS.find((c) => c.name === m[3]);
      if (cmd && pos >= line.from + idx && pos <= line.from + idx + m[3].length) {
        void from;
        return { from: line.from + idx, to: line.from + idx + m[3].length, dom: () => docDom({ title: cmd.name, doc: cmd.doc }) };
      }
    }
  }
  return paramHover(state, pos);
}

/** Problems in `main.cf`: lines that are not `name = value`, and overridden definitions. */
export function mainProblems(text: string): SyntaxProblem[] {
  const problems: SyntaxProblem[] = [];
  const lines = text.split("\n");
  const defs = new Map<string, { line: number; from: number; to: number }[]>();
  let offset = 0;
  let logical = false;
  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i].replace(/\r$/, "");
    const start = offset;
    offset += lines[i].length + 1;
    if (!raw.trim() || raw.trimStart().startsWith("#")) continue;
    if (/^\s/.test(raw)) {
      if (!logical) problems.push({ from: start, to: start + raw.length, message: "Continuation line without a parameter to continue" });
      continue;
    }
    logical = true;
    const m = /^([A-Za-z0-9_][A-Za-z0-9_.-]*)\s*=/.exec(raw);
    if (!m) {
      problems.push({ from: start, to: start + raw.length, message: "Expected `parameter = value`" });
      continue;
    }
    const list = defs.get(m[1]) ?? [];
    list.push({ line: i + 1, from: start, to: start + m[1].length });
    defs.set(m[1], list);
  }
  for (const [name, list] of defs) {
    const last = list[list.length - 1];
    for (const d of list.slice(0, -1)) {
      problems.push({
        from: d.from,
        to: d.to,
        message: `\`${name}\` is defined again on line ${last.line}; the last definition wins`,
        severity: "weak",
      });
    }
  }
  return problems;
}

/** Problems in `master.cf`: service lines need eight valid fields. */
export function masterProblems(text: string): SyntaxProblem[] {
  const problems: SyntaxProblem[] = [];
  let offset = 0;
  for (const raw of text.split("\n")) {
    const start = offset;
    offset += raw.length + 1;
    const line = raw.replace(/\r$/, "");
    if (!line.trim() || line.trimStart().startsWith("#") || /^\s/.test(line)) continue;
    const fields = [...line.matchAll(/\S+/g)];
    const at = (i: number) => ({ from: start + fields[i].index!, to: start + fields[i].index! + fields[i][0].length });
    if (fields.length < 8) {
      problems.push({ from: start, to: start + line.length, message: "A service needs eight fields: service type private unpriv chroot wakeup maxproc command" });
      continue;
    }
    if (!MASTER_TYPES.includes(fields[1][0])) problems.push({ ...at(1), message: `Unknown service type \`${fields[1][0]}\` (expected ${MASTER_TYPES.join(", ")})` });
    for (const i of [2, 3, 4]) if (!/^[yn-]$/.test(fields[i][0])) problems.push({ ...at(i), message: "Expected `y`, `n` or `-`" });
    for (const i of [5, 6]) if (!/^(?:-|\d+\??)$/.test(fields[i][0])) problems.push({ ...at(i), message: "Expected a number or `-`" });
  }
  return problems;
}

export function postfixMainSupport() {
  const lang = streamLanguage(postfixMainParser);
  return [lang, lang.data.of({ autocomplete: mainCompletions }), indentFolding(), hoverDocs(paramHover)];
}

export function postfixMasterSupport() {
  const lang = streamLanguage(postfixMasterParser);
  return [lang, lang.data.of({ autocomplete: masterCompletions }), indentFolding(), hoverDocs(masterHover)];
}

registerLanguage({
  id: "postfix",
  label: "Postfix main.cf",
  extensions: ["cf"],
  fileNames: ["main.cf", "main.cf.proto", "dynamicmaps.cf"],
  indent: "    ",
  load: postfixMainSupport,
});

registerLanguage({
  id: "postfix-master",
  label: "Postfix master.cf",
  fileNames: ["master.cf", "master.cf.proto"],
  indent: "    ",
  load: postfixMasterSupport,
});

registerEditorExtension(({ path }) => {
  const id = languageIdFor(path);
  if (id === "postfix") return syntaxDiagnostics(path, { source: "Postfix" }, (s) => mainProblems(s.doc.toString()));
  if (id === "postfix-master") return syntaxDiagnostics(path, { source: "Postfix" }, (s) => masterProblems(s.doc.toString()));
  return null;
});
