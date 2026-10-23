/**
 * Highlighting of custom file types (Settings | Editor | File Types | +), like PHPStorm's
 * "Custom file type": a generic CodeMirror stream parser built from the definition Rust
 * resolved (`FileTypeInfo.custom`): line and block comments, strings with optional escapes,
 * decimal and hexadecimal numbers with postfixes, four keyword lists and paired brackets.
 */
import type { StreamParser, StringStream } from "@codemirror/language";
import type { StreamLanguage } from "@codemirror/language";
import type { CustomSyntax } from "@/ipc/filetypes";
import { streamLanguage } from "./tokens";

interface State {
  /** Inside a block comment. */
  inBlock: boolean;
}

/** Token names of keyword lists 1–4 (PHPStorm colors each list differently). */
export const KEYWORD_TOKENS = ["keyword", "builtin", "type", "atom"] as const;

const WORD = /^[A-Za-z_$][\w$]*/;

/** The stream parser of a custom file type (exported for tests). */
export function customParser(syntax: CustomSyntax): StreamParser<State> {
  const fold = (w: string) => (syntax.ignoreCase ? w.toLowerCase() : w);
  const lists = syntax.keywords.map((k) => new Set(k));
  // Keywords with characters outside identifiers (`#define`, `@end`), longest first.
  const special = syntax.keywords
    .flatMap((k, i) => k.filter((w) => !/^[A-Za-z_$][\w$]*$/.test(w)).map((w) => ({ w, i })))
    .sort((a, b) => b.w.length - a.w.length);
  const listOf = (word: string) => lists.findIndex((l) => l.has(fold(word)));
  const postfix = (stream: StringStream) => {
    if (syntax.numberPostfixes) stream.eatWhile((c) => syntax.numberPostfixes.includes(c));
  };
  const blockEnd = (stream: StringStream, state: State) => {
    const end = syntax.blockCommentEnd;
    const rest = stream.string.slice(stream.pos);
    const at = end ? rest.indexOf(end) : -1;
    if (at < 0) {
      stream.skipToEnd();
    } else {
      stream.pos += at + end.length;
      state.inBlock = false;
    }
    return "comment";
  };

  return {
    name: "custom",
    startState: () => ({ inBlock: false }),
    copyState: (s) => ({ ...s }),
    token(stream, state) {
      if (state.inBlock) return blockEnd(stream, state);
      if (stream.eatSpace()) return null;
      // Keywords like `#define` win over a `#` line comment.
      for (const { w, i } of special) {
        if (stream.match(w, true, syntax.ignoreCase)) {
          if (!/\w/.test(stream.peek() ?? "")) return KEYWORD_TOKENS[i];
          stream.backUp(w.length);
        }
      }
      const lineStart = stream.string.slice(0, stream.pos).trim() === "";
      if (syntax.lineComment && (!syntax.lineCommentAtStart || lineStart) && stream.match(syntax.lineComment)) {
        stream.skipToEnd();
        return "comment";
      }
      if (syntax.blockCommentStart && stream.match(syntax.blockCommentStart)) {
        state.inBlock = true;
        return blockEnd(stream, state);
      }
      const ch = stream.peek() ?? "";
      if (ch === '"' || ch === "'") {
        stream.next();
        let escaped = false;
        while (!stream.eol()) {
          const c = stream.next();
          if (escaped) escaped = false;
          else if (c === "\\" && syntax.stringEscapes) escaped = true;
          else if (c === ch) break;
        }
        return "string";
      }
      if (syntax.hexPrefix && stream.match(syntax.hexPrefix, true, true)) {
        if (stream.match(/^[0-9a-fA-F]+/)) {
          postfix(stream);
          return "number";
        }
        stream.backUp(syntax.hexPrefix.length);
      }
      if (stream.match(/^\d+(\.\d+)?([eE][+-]?\d+)?/)) {
        postfix(stream);
        return "number";
      }
      const word = stream.match(WORD) as RegExpMatchArray | null;
      if (word) {
        const list = listOf(word[0]);
        return list >= 0 ? KEYWORD_TOKENS[list] : null;
      }
      if (stream.match(/^[{}()[\]]/)) return "bracket";
      if (stream.match(/^[+\-*/%=<>!&|^~?:]+/)) return "operator";
      stream.next();
      return null;
    },
    languageData: {
      commentTokens: {
        ...(syntax.lineComment ? { line: syntax.lineComment } : {}),
        ...(syntax.blockCommentStart && syntax.blockCommentEnd
          ? { block: { open: syntax.blockCommentStart, close: syntax.blockCommentEnd } }
          : {}),
      },
      closeBrackets: {
        brackets: [
          ...(syntax.pairedParens ? ["("] : []),
          ...(syntax.pairedBrackets ? ["["] : []),
          ...(syntax.pairedBraces ? ["{"] : []),
          '"',
          "'",
        ],
      },
    },
  };
}

const languages = new Map<string, StreamLanguage<State>>();

/** The editor language of a custom file type (one per definition, shared by editors). */
export function customFileTypeLanguage(syntax: CustomSyntax): StreamLanguage<State> {
  const key = JSON.stringify(syntax);
  let lang = languages.get(key);
  if (!lang) {
    lang = streamLanguage(customParser(syntax));
    languages.set(key, lang);
  }
  return lang;
}
