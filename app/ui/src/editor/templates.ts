import type { StreamParser, StringStream } from "@codemirror/language";

/**
 * Template languages: Twig, Smarty, Jinja, Latte, Mustache, Handlebars, Nunjucks. One
 * configurable stream parser handles the template tags (keywords, variables, filters,
 * strings, numbers, operators) and hands everything between them to the template's *data
 * language*: HTML by default (built in, so template tags inside attribute values work), or
 * any other stream parser such as LaTeX, SQL, CSS, JavaScript, YAML (PHPStorm's "Template
 * Data Languages").
 */
export interface TemplateSyntax {
  name: string;
  /** Delimiter pairs: [open, close, kind]. Longest openers must come first. */
  delimiters: [string, string, "expr" | "block" | "comment"][];
  keywords: string[];
  /** Built-in functions called without parentheses (`eq .A 1`), styled like function calls. */
  builtins?: string[];
  /** `.Field.Sub` selectors are properties instead of an operator followed by a name. */
  fields?: boolean;
  /** Backtick raw strings, which may span lines (Go). */
  rawStrings?: boolean;
}

interface TemplateState {
  /** Close delimiter of the template tag we are in, if any. */
  inTag: { close: string; kind: "expr" | "block" | "comment"; first: boolean; raw?: boolean } | null;
  /** HTML context outside template tags. */
  html: "text" | "tag" | "comment" | "attrValue";
  quote: string | null;
  tagName: boolean;
  /** State of the data language parser, when it is not the built-in HTML. */
  data: unknown;
}

const OPERATORS = /^(==|!=|<=|>=|\?\?|\?:|\.\.|\*\*|\/\/|[-+*/%~<>=!?:|.])/;

/** Same as CodeMirror's default state copy for stream parsers without `copyState`. */
function copyState<T>(parser: StreamParser<T>, state: T): T {
  if (parser.copyState) return parser.copyState(state);
  if (typeof state !== "object" || state === null) return state;
  const copy = {} as Record<string, unknown>;
  for (const [k, v] of Object.entries(state as Record<string, unknown>)) copy[k] = Array.isArray(v) ? v.slice() : v;
  return copy as T;
}

/**
 * @param data Data language between template tags; `undefined` uses the built-in HTML
 *   tokenizer, `null` treats the content as plain text.
 */
export function templateParser(syntax: TemplateSyntax, data?: StreamParser<unknown> | null): StreamParser<TemplateState> {
  const keywords = new Set(syntax.keywords);
  const builtins = new Set(syntax.builtins);
  const opens = syntax.delimiters.map(([open]) => open);

  /** Position of the next template opener at or after `from`, or the line end. */
  function nextOpen(line: string, from: number): number {
    let next = line.length;
    for (const open of opens) {
      const i = line.indexOf(open, from);
      if (i >= 0 && i < next) next = i;
    }
    return next;
  }

  /** Tokenize data-language content, stopping at the next template tag (CM5's multiplex). */
  function dataToken(stream: StringStream, state: TemplateState): string | null {
    const end = nextOpen(stream.string, stream.pos);
    if (!data) {
      stream.pos = Math.max(end, stream.pos + 1);
      return null;
    }
    const full = stream.string;
    stream.string = full.slice(0, end);
    const start = stream.pos;
    let style: string | null = null;
    try {
      style = (data.token(stream, state.data) as string | null) ?? null;
    } finally {
      stream.string = full;
    }
    if (stream.pos === start) stream.next();
    return style;
  }

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
    if (tag.raw) return rawString(stream, tag);
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
    if (syntax.rawStrings && ch === "`") {
      stream.next();
      return rawString(stream, tag);
    }
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
    if (syntax.fields && stream.match(/^\.[A-Za-z_]\w*/)) return "propertyName";
    if (stream.match(/^[A-Za-z_]\w*/)) {
      const word = stream.current();
      if (stream.peek() === "(" || builtins.has(word)) return "variableName.function";
      if ((first && tag.kind === "block") || keywords.has(word)) return "keyword";
      if (word === "true" || word === "false" || word === "null" || word === "none" || word === "nil") return "atom";
      return "variableName";
    }
    if (stream.match(OPERATORS)) return "operator";
    stream.next();
    return "punctuation";
  }

  /** Rest of a backtick string; it stays open across lines until the closing backtick. */
  function rawString(stream: StringStream, tag: NonNullable<TemplateState["inTag"]>): string {
    tag.raw = !stream.skipTo("`");
    if (!tag.raw) stream.next();
    else stream.skipToEnd();
    return "string";
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
    startState: (indentUnit) => ({
      inTag: null,
      html: "text",
      quote: null,
      tagName: false,
      data: data ? data.startState?.(indentUnit) ?? {} : null,
    }),
    copyState: (s) => ({ ...s, inTag: s.inTag && { ...s.inTag }, data: data ? copyState(data, s.data) : null }),
    token(stream, state) {
      if (state.inTag) return inTemplateTag(stream, state);
      if (data !== undefined || state.html !== "comment") {
        const style = openTag(stream, state);
        if (style) return style;
      }
      return data === undefined ? html(stream, state) : dataToken(stream, state);
    },
    blankLine(state, indentUnit) {
      if (!state.inTag && data?.blankLine) data.blankLine(state.data, indentUnit);
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

/** Smarty with configurable delimiters (`$smarty->left_delimiter`), e.g. `<{ }>` for LaTeX. */
export function smartySyntaxWith(left = "{", right = "}"): TemplateSyntax {
  return {
    ...smartySyntax,
    delimiters: [
      [`${left}*`, `*${right}`, "comment"],
      [`${left}/`, right, "block"],
      [left, right, "block"],
    ],
  };
}

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

/**
 * Go `text/template` and `html/template`: `{{ }}` actions with `{{-` / `-}}` trim markers,
 * block comments, `.Field` selectors, `$variables`, pipelines and builtin functions.
 */
export const goTemplateSyntax: TemplateSyntax = {
  name: "gotemplate",
  delimiters: [
    ["{{- /*", "*/ -}}", "comment"],
    ["{{/*", "*/}}", "comment"],
    ["{{", "}}", "expr"],
  ],
  keywords: ["if", "else", "end", "range", "with", "define", "template", "block", "break", "continue"],
  builtins: [
    "and", "or", "not", "len", "index", "slice", "print", "printf", "println", "html", "js", "urlquery",
    "call", "eq", "ne", "lt", "le", "gt", "ge",
  ],
  fields: true,
  rawStrings: true,
};
