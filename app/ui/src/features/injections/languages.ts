/**
 * Syntax of injected languages: the parser that colors an injected range in the host editor
 * (per keystroke, so it stays in TypeScript) and the full editor support of the Edit
 * Fragment editor. Which language a string gets is decided in Rust (`forge-injections`).
 */
import { StreamLanguage, type Language, type StringStream } from "@codemirror/language";
import type { Extension } from "@codemirror/state";
import { sql, MySQL } from "@codemirror/lang-sql";
import { htmlLanguage } from "@codemirror/lang-html";
import { cssLanguage } from "@codemirror/lang-css";
import { jsonLanguage } from "@codemirror/lang-json";
import { javascriptLanguage } from "@codemirror/lang-javascript";
import { xmlLanguage } from "@codemirror/lang-xml";
import { tags as t, tagHighlighter, highlightTree } from "@lezer/highlight";
import { dataLanguageSupport } from "@/editor/templateData";

// ---------------------------------------------------------------- RegExp

interface RegexpState {
  /** Inside a character class `[…]`. */
  inClass: boolean;
}

/** Regular expressions (PCRE and JavaScript): escapes, classes, groups, quantifiers, anchors. */
export const regexpParser = {
  name: "regexp",
  startState: (): RegexpState => ({ inClass: false }),
  token(stream: StringStream, state: RegexpState): string | null {
    if (stream.match(/^\\(?:[pP]\{[^}]*\}|x\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|u[0-9a-fA-F]{4}|c[A-Za-z]|k<[^>]*>|\d+|.)/)) return "escape";
    if (state.inClass) {
      if (stream.eat("]")) {
        state.inClass = false;
        return "regexpClass";
      }
      if (stream.match(/^\[:\^?\w+:\]/)) return "regexpClass";
      stream.next();
      return "regexpClassBody";
    }
    if (stream.match(/^\[\^?/)) {
      state.inClass = true;
      return "regexpClass";
    }
    if (stream.match(/^\(\?(?:<[=!]|[:=!>|]|P?<\w+>|'\w+'|[imsxnU-]+:?|#[^)]*)?/)) return "regexpGroup";
    if (stream.match(/^\((?!\?)/) || stream.eat(")")) return "regexpGroup";
    if (stream.match(/^(?:[*+?]|\{\d+(?:,\d*)?\})[?+]?/)) return "regexpQuantifier";
    if (stream.eat(/[|^$.]/)) return "regexpAnchor";
    stream.next();
    return null;
  },
  tokenTable: {
    escape: t.escape,
    regexpClass: t.bracket,
    regexpClassBody: t.string,
    regexpGroup: t.bracket,
    regexpQuantifier: t.number,
    regexpAnchor: t.keyword,
  },
};

export const regexpLanguage = StreamLanguage.define(regexpParser);

// ---------------------------------------------------------------- languages

let sqlLanguage: Language | null = null;

/** The language that parses an injected range, `null` for unknown ids. */
export function injectedLanguage(id: string): Language | null {
  switch (id) {
    case "sql":
      return (sqlLanguage ??= sql({ dialect: MySQL }).language);
    case "html":
      return htmlLanguage;
    case "css":
      return cssLanguage;
    case "json":
      return jsonLanguage;
    case "javascript":
      return javascriptLanguage;
    case "xml":
      return xmlLanguage;
    case "regexp":
      return regexpLanguage;
    default:
      return null;
  }
}

/** The editor support of the Edit Fragment editor. */
export function fragmentLanguageSupport(id: string): Extension {
  return id === "regexp" ? regexpLanguage : dataLanguageSupport(id);
}

// ---------------------------------------------------------------- tokens

/**
 * Token classes of injected text. Like `forgeHighlightStyle`, but as classes that override
 * the host string's color (see `injections.css`).
 */
export const injectedHighlighter = tagHighlighter([
  { tag: [t.keyword, t.modifier, t.controlKeyword, t.operatorKeyword, t.definitionKeyword], class: "cm-inj-keyword" },
  { tag: [t.string, t.special(t.string), t.attributeValue], class: "cm-inj-string" },
  { tag: [t.number, t.bool, t.null, t.atom, t.unit, t.color], class: "cm-inj-number" },
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], class: "cm-inj-comment" },
  { tag: [t.variableName, t.special(t.variableName)], class: "cm-inj-variable" },
  { tag: [t.function(t.variableName), t.function(t.propertyName), t.standard(t.variableName)], class: "cm-inj-function" },
  { tag: [t.className, t.typeName, t.namespace], class: "cm-inj-type" },
  { tag: [t.propertyName, t.attributeName, t.meta, t.annotation], class: "cm-inj-attribute" },
  { tag: t.tagName, class: "cm-inj-tag" },
  { tag: [t.operator, t.punctuation, t.bracket, t.angleBracket], class: "cm-inj-operator" },
  { tag: t.escape, class: "cm-inj-escape" },
  { tag: t.invalid, class: "cm-inj-invalid" },
]);

export interface TokenSpan {
  from: number;
  to: number;
  cls: string;
}

const cache = new Map<string, TokenSpan[]>();
const CACHE_SIZE = 300;

/** Token spans of `text` in language `id` (cached: the same fragments are drawn on every keystroke). */
export function tokenize(id: string, text: string): TokenSpan[] {
  const key = `${id}\u0000${text}`;
  const hit = cache.get(key);
  if (hit) {
    // Keep recently used entries.
    cache.delete(key);
    cache.set(key, hit);
    return hit;
  }
  const lang = injectedLanguage(id);
  const out: TokenSpan[] = [];
  if (lang) {
    const tree = lang.parser.parse(text);
    highlightTree(tree, injectedHighlighter, (from, to, cls) => out.push({ from, to, cls }));
  }
  cache.set(key, out);
  if (cache.size > CACHE_SIZE) cache.delete(cache.keys().next().value as string);
  return out;
}
