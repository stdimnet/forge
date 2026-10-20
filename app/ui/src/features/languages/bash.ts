/**
 * Bash: highlighting for scripts and dotfiles (keywords, builtins, expansions, heredocs,
 * nested `$(...)`), completion of keywords, builtins, functions and variables, and
 * indentation folding.
 */
import type { StreamParser, StringStream } from "@codemirror/language";
import type { CompletionContext } from "@codemirror/autocomplete";
import { registerLanguage } from "@/editor/languages";
import { collectNames, indentFolding, streamLanguage } from "./shared";

export const KEYWORDS = [
  "if", "then", "elif", "else", "fi", "for", "while", "until", "do", "done", "case", "esac", "in",
  "select", "function", "time", "coproc", "!", "[[", "]]", "{", "}",
];

/** Keywords after which a new command starts. */
const COMMAND_KEYWORDS = new Set(["if", "then", "elif", "else", "while", "until", "do", "time", "coproc", "!", "{"]);

export const BUILTINS = [
  "alias", "bg", "bind", "break", "builtin", "caller", "cd", "command", "compgen", "complete", "compopt",
  "continue", "declare", "dirs", "disown", "echo", "enable", "eval", "exec", "exit", "export", "false",
  "fc", "fg", "getopts", "hash", "help", "history", "jobs", "kill", "let", "local", "logout", "mapfile",
  "popd", "printf", "pushd", "pwd", "read", "readarray", "readonly", "return", "set", "shift", "shopt",
  "source", "suspend", "test", "times", "trap", "true", "type", "typeset", "ulimit", "umask", "unalias",
  "unset", "wait", ".", ":",
];

const KEYWORD_SET = new Set(KEYWORDS);
const CONTROL_SET = new Set(["if", "then", "elif", "else", "fi", "case", "esac", "while", "until", "for", "do", "done", "select"]);
const BUILTIN_SET = new Set(BUILTINS);

export const SPECIAL_VARIABLES = [
  "BASH", "BASH_SOURCE", "BASH_VERSION", "BASHPID", "EUID", "FUNCNAME", "GROUPS", "HOME", "HOSTNAME",
  "IFS", "LINENO", "OLDPWD", "OPTARG", "OPTIND", "PATH", "PIPESTATUS", "PPID", "PS1", "PWD", "RANDOM",
  "REPLY", "SECONDS", "SHELL", "SHLVL", "UID", "USER", "LANG", "TERM", "TMPDIR", "EDITOR", "@", "*", "#",
  "?", "$", "!", "0", "1", "2", "3", "-", "_",
];

type Frame = '"' | "(" | "((" | "{" | "`";

interface Heredoc {
  tag: string;
  strip: boolean;
  expand: boolean;
}

interface State {
  /** Open `"`, `$(`, `$((`, `${` and backtick contexts, innermost last. */
  stack: Frame[];
  /** Inside a single-quoted or `$'...'` string spanning lines. */
  single: boolean;
  /** The next word starts a command (so it may be a keyword, builtin or function name). */
  command: boolean;
  /** The next word is the name in `function name`. */
  functionName: boolean;
  /** Heredocs introduced on the current line, whose bodies follow it. */
  pending: Heredoc[];
  heredoc: Heredoc | null;
}

const WORD_END = /[\s;&|<>()$`"'\\#={}]/;

function readWord(stream: StringStream): string {
  const start = stream.pos;
  while (!stream.eol() && !WORD_END.test(stream.peek()!)) stream.next();
  return stream.string.slice(start, stream.pos);
}

function expansion(stream: StringStream, state: State): string {
  // `$` already consumed.
  if (stream.match("((")) {
    state.stack.push("((");
    state.command = false;
    return "operator";
  }
  if (stream.eat("(")) {
    state.stack.push("(");
    state.command = true;
    return "operator";
  }
  if (stream.eat("{")) {
    state.stack.push("{");
    state.command = false;
    stream.match(/^[!#]?/);
    stream.match(/^(?:[A-Za-z_]\w*|\d+|[@*#?$!-])/);
    return "variable";
  }
  if (stream.eat("'")) {
    state.single = true;
    return singleQuoted(stream, state, true);
  }
  if (stream.match(/^(?:[A-Za-z_]\w*|\d|[@*#?$!-])/)) return "variable";
  return "operator";
}

function singleQuoted(stream: StringStream, state: State, ansi = false): string {
  let escaped = false;
  while (!stream.eol()) {
    const ch = stream.next();
    if (ansi && ch === "\\" && !escaped) escaped = true;
    else if (ch === "'" && !escaped) {
      state.single = false;
      return "string";
    } else escaped = false;
  }
  return "string";
}

function doubleQuoted(stream: StringStream, state: State): string | null {
  const ch = stream.peek();
  if (ch === '"') {
    stream.next();
    state.stack.pop();
    return "string";
  }
  if (ch === "$" && /[({A-Za-z_\d@*#?$!-]/.test(stream.string.charAt(stream.pos + 1))) {
    stream.next();
    return expansion(stream, state);
  }
  if (ch === "`") {
    stream.next();
    state.stack.push("`");
    state.command = true;
    return "operator";
  }
  if (ch === "\\") {
    stream.next();
    stream.next();
    return "escape";
  }
  while (!stream.eol() && !/["$`\\]/.test(stream.peek()!)) stream.next();
  if (stream.pos === stream.start) stream.next();
  return "string";
}

function heredocLine(stream: StringStream, state: State): string | null {
  const hd = state.heredoc!;
  if (stream.sol()) {
    const text = hd.strip ? stream.string.replace(/^\t+/, "") : stream.string;
    if (text === hd.tag) {
      stream.skipToEnd();
      state.heredoc = null;
      return "keyword";
    }
  }
  if (hd.expand && stream.peek() === "$" && /[({A-Za-z_\d@*#?$!-]/.test(stream.string.charAt(stream.pos + 1))) {
    stream.next();
    return expansion(stream, state);
  }
  if (hd.expand && stream.peek() === "\\") {
    stream.next();
    stream.next();
    return "escape";
  }
  while (!stream.eol() && !(hd.expand && /[$\\]/.test(stream.peek()!))) stream.next();
  if (stream.pos === stream.start) stream.next();
  return "string";
}

const COMMAND_SEPARATORS = new Set(["&&", "||", ";;&", ";;", ";&", "|", "|&", "&", ";"]);
const OPERATORS = ["&&", "||", ";;&", ";;", ";&", "|&", "&>>", "&>", ">>", ">&", "<&", ">|", "<>", "<<<", ">", "<", "|", "&", ";", "=~", "==", "!=", "+=", "="];

export const bashParser: StreamParser<State> = {
  name: "bash",
  startState: () => ({ stack: [], single: false, command: true, functionName: false, pending: [], heredoc: null }),
  copyState: (s) => ({ ...s, stack: [...s.stack], pending: [...s.pending] }),
  token(stream, state) {
    if (stream.sol()) {
      if (state.pending.length && !state.heredoc) state.heredoc = state.pending.shift()!;
      if (!state.single && !state.heredoc && state.stack.at(-1) !== '"') state.command = true;
    }
    if (state.heredoc) return heredocLine(stream, state);
    if (state.single) return singleQuoted(stream, state);
    if (state.stack.at(-1) === '"') return doubleQuoted(stream, state);
    if (stream.eatSpace()) return null;

    const ch = stream.peek()!;
    const top = state.stack.at(-1);

    if (ch === "#") {
      if (stream.sol() && stream.match("#!")) {
        stream.skipToEnd();
        return "meta";
      }
      stream.skipToEnd();
      return "comment";
    }
    if (ch === "\\") {
      stream.next();
      stream.next();
      return "escape";
    }
    if (ch === "'") {
      stream.next();
      state.single = true;
      state.command = false;
      return singleQuoted(stream, state);
    }
    if (ch === '"') {
      stream.next();
      state.stack.push('"');
      state.command = false;
      return "string";
    }
    if (ch === "`") {
      stream.next();
      if (top === "`") state.stack.pop();
      else {
        state.stack.push("`");
        state.command = true;
      }
      return "operator";
    }
    if (ch === "$") {
      stream.next();
      state.command = false;
      return expansion(stream, state);
    }
    if (ch === "(") {
      stream.next();
      if (stream.eat("(")) state.stack.push("((");
      state.command = true;
      return "bracket";
    }
    if (ch === ")") {
      stream.next();
      if (top === "(") state.stack.pop();
      else if (top === "((") {
        stream.eat(")");
        state.stack.pop();
      }
      state.command = false;
      return "bracket";
    }
    if (ch === "}" && top === "{") {
      stream.next();
      state.stack.pop();
      return "variable";
    }
    if (ch === "{" || ch === "}") {
      stream.next();
      const isKeyword = state.command || ch === "}";
      state.command = ch === "{";
      return isKeyword ? "keyword" : "operator";
    }
    // Heredoc: `<<EOF`, `<<-'EOF'`, `<<"EOF"`.
    const heredoc = stream.match(/^<<(-?)\s*(?:'([^']*)'|"([^"]*)"|\\?([^\s;&|<>()]+))/) as RegExpMatchArray | null;
    if (heredoc) {
      const quoted = heredoc[2] !== undefined || heredoc[3] !== undefined || stream.string.slice(0, stream.pos).endsWith("\\" + heredoc[4]);
      state.pending.push({ tag: heredoc[2] ?? heredoc[3] ?? heredoc[4], strip: heredoc[1] === "-", expand: !quoted });
      return "special";
    }
    for (const op of OPERATORS) {
      if (!stream.match(op)) continue;
      state.command = COMMAND_SEPARATORS.has(op);
      return "operator";
    }
    // Words.
    const word = readWord(stream);
    if (!word) {
      stream.next();
      return null;
    }
    const wasCommand = state.command;
    state.command = false;
    if (state.functionName) {
      state.functionName = false;
      return "func";
    }
    if (wasCommand && /^\+?=/.test(stream.string.slice(stream.pos)) && /^[A-Za-z_]\w*(?:\[[^\]]*\])?$/.test(word)) {
      state.command = true;
      return "def";
    }
    if (wasCommand && /^\s*\(\s*\)/.test(stream.string.slice(stream.pos))) return "func";
    if (/^-{1,2}[A-Za-z0-9][\w-]*$/.test(word) || word === "--") return "option";
    if (/^\d+$/.test(word)) return "number";
    if (KEYWORD_SET.has(word) && (wasCommand || word === "in" || word === "do" || word === "then" || word === "[[" || word === "]]")) {
      if (word === "function") state.functionName = true;
      state.command = COMMAND_KEYWORDS.has(word);
      return CONTROL_SET.has(word) ? "control" : "keyword";
    }
    if (wasCommand && BUILTIN_SET.has(word)) return "builtin";
    if (word === "true" || word === "false") return "atom";
    return null;
  },
  languageData: {
    commentTokens: { line: "#" },
    closeBrackets: { brackets: ["(", "[", "{", '"', "'", "`"] },
    indentOnInput: /^\s*(?:fi|done|esac|else|elif|then|do|\}|\)|;;)$/,
    wordChars: "_-",
  },
};

function completions(ctx: CompletionContext) {
  const dollar = ctx.matchBefore(/\$\{?[\w@*#?$!-]*/);
  const names = [...collectNames(ctx.state, /^\s*(?:export\s+|declare\s+(?:-\w+\s+)?|local\s+|readonly\s+)?([A-Za-z_]\w*)\+?=/gm)];
  if (dollar) {
    const braces = dollar.text.startsWith("${");
    const from = dollar.from + (braces ? 2 : 1);
    const vars = [...new Set([...names, ...SPECIAL_VARIABLES])];
    return {
      from,
      options: vars.map((label) => ({ label, type: "variable", boost: names.includes(label) ? 5 : 0 })),
      validFor: /^[\w@*#?$!-]*$/,
    };
  }
  const word = ctx.matchBefore(/[\w.:-]+/);
  if (!word || (word.from === word.to && !ctx.explicit)) return null;
  const before = ctx.state.doc.sliceString(Math.max(0, word.from - 40), word.from);
  const atCommand = /(?:^|[;&|(`{]|\b(?:then|do|else|elif|if|while|until|time)|\$\()\s*$/.test(before.replace(/^[\s\S]*\n/, ""));
  if (!atCommand) return null;
  const functions = collectNames(ctx.state, /^\s*(?:function\s+)?([A-Za-z_][\w:-]*)\s*\(\s*\)/gm);
  return {
    from: word.from,
    options: [
      ...KEYWORDS.filter((k) => /^\w+$/.test(k)).map((label) => ({ label, type: "keyword" })),
      ...BUILTINS.filter((b) => /^\w+$/.test(b)).map((label) => ({ label, type: "function", detail: "builtin" })),
      ...functions.map((label) => ({ label, type: "function", detail: "function", boost: 5 })),
    ],
    validFor: /^[\w.:-]*$/,
  };
}

export function bashSupport() {
  const lang = streamLanguage(bashParser);
  return [lang, lang.data.of({ autocomplete: completions }), indentFolding()];
}

registerLanguage({
  id: "shell",
  label: "Shell Script",
  extensions: ["sh", "bash", "zsh", "ksh", "bats"],
  fileNames: [".bashrc", ".bash_profile", ".bash_login", ".bash_logout", ".bash_aliases", ".profile", ".zshrc", ".zprofile", ".envrc", "PKGBUILD", ".xinitrc", ".xprofile"],
  shebang: /^#!.*\b(?:ba|z|k|da|a)?sh\b/,
  indent: "  ",
  load: bashSupport,
});
