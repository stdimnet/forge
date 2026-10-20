import type { StreamParser, StringStream } from "@codemirror/language";

/**
 * Template languages that embed tags in HTML: Twig, Smarty, Jinja, Latte, Mustache,
 * Handlebars, Nunjucks. One configurable stream parser handles the HTML around the template
 * tags (elements, attributes, comments) and the tags themselves (keywords, variables,
 * filters, strings, numbers, operators).
 */
export interface TemplateSyntax {
  name: string;
  /** Delimiter pairs: [open, close, kind]. Longest openers must come first. */
  delimiters: [string, string, "expr" | "block" | "comment"][];
  keywords: string[];
}

interface TemplateState {
  /** Close delimiter of the template tag we are in, if any. */
  inTag: { close: string; kind: "expr" | "block" | "comment"; first: boolean } | null;
  /** HTML context outside template tags. */
  html: "text" | "tag" | "comment" | "attrValue";
  quote: string | null;
  tagName: boolean;
}

const OPERATORS = /^(==|!=|<=|>=|\?\?|\?:|\.\.|\*\*|\/\/|[-+*/%~<>=!?:|.])/;

export function templateParser(syntax: TemplateSyntax): StreamParser<TemplateState> {
  const keywords = new Set(syntax.keywords);

  function openTag(stream: StringStream, state: TemplateState): string | null {
    for (const [open, close, kind] of syntax.delimiters) {
      if (stream.match(open)) {
        // Whitespace-control modifiers like {%- or {{~.
        stream.match(/^[-~]/);
        state.inTag = { close, kind, first: true };
        return kind === "comment" ? "comment" : "meta";
      }
    }
    return null;
  }

  function inTemplateTag(stream: StringStream, state: TemplateState): string | null {
    const tag = state.inTag!;
    if (tag.kind === "comment") {
      const end = stream.string.indexOf(tag.close, stream.pos);
      if (end < 0) {
        stream.skipToEnd();
      } else {
        stream.pos = end + tag.close.length;
        state.inTag = null;
      }
      return "comment";
    }
    if (stream.match(new RegExp(`^[-~]?${escape(tag.close)}`))) {
      state.inTag = null;
      return "meta";
    }
    if (stream.eatSpace()) return null;
    const first = tag.first;
    tag.first = false;

    const ch = stream.peek();
    if (ch === '"' || ch === "'") {
      stream.next();
      let escaped = false;
      let c: string | void;
      while ((c = stream.next()) != null) {
        if (c === ch && !escaped) break;
        escaped = !escaped && c === "\\";
      }
      return "string";
    }
    if (stream.match(/^\d+(\.\d+)?/)) return "number";
    if (stream.match(/^\$[A-Za-z_]\w*/)) return "variableName";
    if (stream.match(/^\|\s*[A-Za-z_]\w*/)) return "variableName.function";
    if (stream.match(/^[A-Za-z_]\w*/)) {
      const word = stream.current();
      if (stream.peek() === "(") return "variableName.function";
      if ((first && tag.kind === "block") || keywords.has(word)) return "keyword";
      if (word === "true" || word === "false" || word === "null" || word === "none") return "atom";
      return "variableName";
    }
    if (stream.match(OPERATORS)) return "operator";
    stream.next();
    return "punctuation";
  }

  function html(stream: StringStream, state: TemplateState): string | null {
    if (state.html === "comment") {
      if (stream.skipTo("-->")) {
        stream.match("-->");
        state.html = "text";
      } else stream.skipToEnd();
      return "comment";
    }
    if (state.html === "attrValue") {
      while (!stream.eol()) {
        if (syntax.delimiters.some(([open]) => stream.string.startsWith(open, stream.pos))) return "attributeValue";
        if (stream.next() === state.quote) {
          state.html = "tag";
          state.quote = null;
          break;
        }
      }
      return "attributeValue";
    }
    if (state.html === "tag") {
      if (stream.eatSpace()) return null;
      if (stream.match(/^\/?>/)) {
        state.html = "text";
        return "angleBracket";
      }
      if (state.tagName && stream.match(/^[\w:-]+/)) {
        state.tagName = false;
        return "tagName";
      }
      if (stream.match(/^[^\s=>/"'{]+/)) return "attributeName";
      if (stream.eat("=")) return "operator";
      const q = stream.peek();
      if (q === '"' || q === "'") {
        stream.next();
        state.quote = q;
        state.html = "attrValue";
        return "attributeValue";
      }
      stream.next();
      return null;
    }
    // Text content.
    if (stream.match("<!--")) {
      state.html = "comment";
      return "comment";
    }
    if (stream.match(/^<!doctype[^>]*>/i)) return "meta";
    if (stream.match(/^<\/?(?=[A-Za-z])/)) {
      state.html = "tag";
      state.tagName = true;
      return "angleBracket";
    }
    if (stream.match(/^&\w+;/)) return "character";
    // Plain text up to the next tag or template delimiter.
    stream.next();
    while (!stream.eol()) {
      const rest = stream.string.slice(stream.pos);
      if (rest.startsWith("<") || rest.startsWith("&") || syntax.delimiters.some(([open]) => rest.startsWith(open))) break;
      stream.next();
    }
    return null;
  }

  return {
    name: syntax.name,
    startState: () => ({ inTag: null, html: "text", quote: null, tagName: false }),
    copyState: (s) => ({ ...s, inTag: s.inTag && { ...s.inTag } }),
    token(stream, state) {
      if (state.inTag) return inTemplateTag(stream, state);
      if (state.html !== "comment") {
        const style = openTag(stream, state);
        if (style) return style;
      }
      return html(stream, state);
    },
  };
}

function escape(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

const JINJA_KEYWORDS = [
  "if", "elif", "else", "endif", "for", "endfor", "in", "is", "not", "and", "or", "set", "endset",
  "block", "endblock", "extends", "include", "import", "from", "as", "with", "endwith", "macro",
  "endmacro", "call", "endcall", "filter", "endfilter", "raw", "endraw", "autoescape", "endautoescape",
];

export const twigSyntax: TemplateSyntax = {
  name: "twig",
  delimiters: [
    ["{#", "#}", "comment"],
    ["{{", "}}", "expr"],
    ["{%", "%}", "block"],
  ],
  keywords: [
    ...JINJA_KEYWORDS, "embed", "endembed", "use", "only", "ignore", "missing", "apply", "endapply",
    "verbatim", "endverbatim", "sandbox", "endsandbox", "spaceless", "endspaceless", "trans", "endtrans",
    "form_theme", "matches", "starts", "ends", "with", "defined", "empty", "same", "divisible", "by",
  ],
};

export const jinjaSyntax: TemplateSyntax = { ...twigSyntax, name: "jinja", keywords: JINJA_KEYWORDS };

export const smartySyntax: TemplateSyntax = {
  name: "smarty",
  delimiters: [
    ["{*", "*}", "comment"],
    ["{/", "}", "block"],
    ["{", "}", "block"],
  ],
  keywords: [
    "if", "elseif", "else", "foreach", "foreachelse", "for", "while", "section", "sectionelse", "include",
    "assign", "capture", "function", "block", "extends", "literal", "nocache", "strip", "as", "to", "step",
    "eq", "ne", "neq", "gt", "lt", "gte", "ge", "lte", "le", "not", "and", "or", "mod", "is", "even", "odd",
  ],
};

export const latteSyntax: TemplateSyntax = {
  name: "latte",
  delimiters: [
    ["{*", "*}", "comment"],
    ["{/", "}", "block"],
    ["{", "}", "block"],
  ],
  keywords: [
    "if", "elseif", "else", "ifset", "foreach", "for", "while", "include", "layout", "block", "define",
    "var", "default", "capture", "first", "last", "sep", "snippet", "control", "link", "plink", "as",
  ],
};

export const mustacheSyntax: TemplateSyntax = {
  name: "handlebars",
  delimiters: [
    ["{{!--", "--}}", "comment"],
    ["{{!", "}}", "comment"],
    ["{{{", "}}}", "expr"],
    ["{{#", "}}", "block"],
    ["{{/", "}}", "block"],
    ["{{^", "}}", "block"],
    ["{{>", "}}", "block"],
    ["{{", "}}", "expr"],
  ],
  keywords: ["if", "unless", "each", "with", "else", "as", "this", "lookup", "log"],
};
