import type { StreamParser, StringStream } from "@codemirror/language";

/**
 * Template languages: Twig, Smarty, Jinja, Latte, Mustache, Handlebars, Nunjucks. One
 * configurable stream parser handles the template tags (keywords, variables, filters,
 * strings, numbers, operators) and hands everything between them to the template's *data
 * language*: HTML by default (built in, so template tags inside attribute values work), or
 * any other stream parser such as LaTeX, SQL, CSS, JavaScript, YAML (PHPStorm's "Template
 * Data Languages").
 */
export type TagKind = "expr" | "block" | "end" | "comment";

export interface TemplateSyntax {
  name: string;
  /**
   * Delimiter pairs: [open, close, kind]. Longest openers must come first. `end` opens a
   * closing block tag such as Smarty's `{/if}` or Handlebars' `{{/each}}`.
   */
  delimiters: [string, string, TagKind][];
  keywords: string[];
  /** Built-in functions called without parentheses (`eq .A 1`), styled like function calls. */
  builtins?: string[];
  /** `.Field.Sub` selectors are properties instead of an operator followed by a name. */
  fields?: boolean;
  /** Backtick raw strings, which may span lines (Go). */
  rawStrings?: boolean;
  /**
   * Smarty's `auto_literal`: an opener followed by whitespace (`{ ` in JavaScript or CSS) is
   * text, not a tag.
   */
  autoLiteral?: boolean;
  /**
   * Blocks whose content is not parsed, like Smarty's `{literal}`: block name → the closing
   * tag that ends it, built from the configured delimiters (`[[/literal]]`).
   */
  literalBlocks?: Record<string, string>;
  /**
   * Block tags that start with this prefix close the block of the same name (`{% endif %}`
   * closes `{% if %}`), for folding and matching.
   */
  endPrefix?: string;
}

interface TemplateState {
  /** Close delimiter of the template tag we are in, if any. */
  inTag: { close: string; kind: TagKind; first: boolean; raw?: boolean; name?: string } | null;
  /** Closing tag of the literal block we are in; its content is plain data language. */
  literal: string | null;
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

/** The delimiter that opens a tag at `pos` in `line`, if any. Auto-literal openers do not. */
export function openerAt(syntax: TemplateSyntax, line: string, pos: number): [string, string, TagKind] | undefined {
  for (const d of syntax.delimiters) {
    if (!line.startsWith(d[0], pos)) continue;
    const after = line.charAt(pos + d[0].length);
    if (syntax.autoLiteral && d[2] !== "comment" && (after === "" || /\s/.test(after))) return undefined;
    return d;
  }
  return undefined;
}

/**
 * @param data Data language between template tags; `undefined` uses the built-in HTML
 *   tokenizer, `null` treats the content as plain text.
 */
export function templateParser(syntax: TemplateSyntax, data?: StreamParser<unknown> | null): StreamParser<TemplateState> {
  const keywords = new Set(syntax.keywords);
  const builtins = new Set(syntax.builtins);
  const opens = [...new Set(syntax.delimiters.map(([open]) => open))];

  /** Position of the next template opener (or literal block end) at or after `from`, or the line end. */
  function nextOpen(line: string, from: number, literal: string | null): number {
    if (literal) {
      const i = line.indexOf(literal, from);
      return i < 0 ? line.length : i;
    }
    let next = line.length;
    for (const open of opens) {
      let i = line.indexOf(open, from);
      while (i >= 0 && i < next && !openerAt(syntax, line, i)) i = line.indexOf(open, i + 1);
      if (i >= 0 && i < next) next = i;
    }
    return next;
  }

  /** Tokenize data-language content, stopping at the next template tag (CM5's multiplex). */
  function dataToken(stream: StringStream, state: TemplateState): string | null {
    const end = nextOpen(stream.string, stream.pos, state.literal);
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
    if (state.literal) {
      // Inside a literal block only its closing tag is a tag again.
      if (!stream.string.startsWith(state.literal, stream.pos)) return null;
      state.literal = null;
    }
    const d = openerAt(syntax, stream.string, stream.pos);
    if (!d) return null;
    const [open, close, kind] = d;
    stream.pos += open.length;
    // Whitespace-control modifiers like {%- or {{~.
    stream.match(/^[-~]/);
    state.inTag = { close, kind, first: true };
    return kind === "comment" ? "comment" : "meta";
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
      const literalEnd = tag.kind === "block" && tag.name ? syntax.literalBlocks?.[tag.name] : undefined;
      if (literalEnd) state.literal = literalEnd;
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
      if (first && (tag.kind === "block" || tag.kind === "end")) {
        tag.name = word;
        return "keyword";
      }
      if (keywords.has(word)) return "keyword";
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
        if (openerAt(syntax, stream.string, stream.pos)) return "attributeValue";
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
      if (rest.startsWith("<") || rest.startsWith("&") || openerAt(syntax, stream.string, stream.pos)) break;
      stream.next();
    }
    return null;
  }

  /** Literal block content with the built-in HTML: plain text up to the closing tag. */
  function literalHtml(stream: StringStream, state: TemplateState): string | null {
    const style = openTag(stream, state);
    if (style) return style;
    stream.pos = Math.max(nextOpen(stream.string, stream.pos, state.literal), stream.pos + 1);
    return null;
  }

  return {
    name: syntax.name,
    languageData: data?.languageData,
    startState: (indentUnit) => ({
      inTag: null,
      literal: null,
      html: "text",
      quote: null,
      tagName: false,
      data: data ? data.startState?.(indentUnit) ?? {} : null,
    }),
    copyState: (s) => ({ ...s, inTag: s.inTag && { ...s.inTag }, data: data ? copyState(data, s.data) : null }),
    token(stream, state) {
      if (state.inTag) return inTemplateTag(stream, state);
      if (state.literal && data === undefined) return literalHtml(stream, state);
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

const TWIG_KEYWORDS = [
  ...JINJA_KEYWORDS, "embed", "endembed", "use", "only", "ignore", "missing", "apply", "endapply",
  "verbatim", "endverbatim", "sandbox", "endsandbox", "spaceless", "endspaceless", "trans", "endtrans",
  "form_theme", "matches", "starts", "ends", "with", "defined", "empty", "same", "divisible", "by",
  // Shopware 6 storefront tags.
  "sw_extends", "sw_include", "sw_icon", "sw_thumbnails", "sw_silent_feature_call", "endsw_silent_feature_call",
];

/** Twig's `tag_variable`, `tag_block` and `tag_comment` (Jinja's `*_start_string` / `*_end_string`). */
export interface TwigDelimiters {
  variableStart?: string;
  variableEnd?: string;
  blockStart?: string;
  blockEnd?: string;
  commentStart?: string;
  commentEnd?: string;
}

function twigDelimiters(d: TwigDelimiters): TemplateSyntax["delimiters"] {
  const list: TemplateSyntax["delimiters"] = [
    [d.commentStart || "{#", d.commentEnd || "#}", "comment"],
    [d.variableStart || "{{", d.variableEnd || "}}", "expr"],
    [d.blockStart || "{%", d.blockEnd || "%}", "block"],
  ];
  // Longest openers first, so a longer opener is not taken for a shorter one.
  return list.sort((a, b) => b[0].length - a[0].length);
}

/** Twig with the given delimiters, e.g. `\VAR{ }`, `\BLOCK{ }` and `\#{ }` for LaTeX. */
export function twigSyntaxWith(d: TwigDelimiters = {}): TemplateSyntax {
  return { name: "twig", delimiters: twigDelimiters(d), keywords: TWIG_KEYWORDS, endPrefix: "end" };
}

export function jinjaSyntaxWith(d: TwigDelimiters = {}): TemplateSyntax {
  return { name: "jinja", delimiters: twigDelimiters(d), keywords: JINJA_KEYWORDS, endPrefix: "end" };
}

export const twigSyntax: TemplateSyntax = twigSyntaxWith();

/** Django's built-in tags and their operators (`{% if a in b %}`, `{% for x in y %}`). */
const DJANGO_KEYWORDS = [
  "autoescape", "endautoescape", "block", "endblock", "comment", "endcomment", "csrf_token", "cycle", "debug",
  "extends", "filter", "endfilter", "firstof", "for", "empty", "endfor", "if", "elif", "else", "endif",
  "ifchanged", "endifchanged", "include", "load", "lorem", "now", "regroup", "resetcycle", "spaceless",
  "endspaceless", "templatetag", "url", "verbatim", "endverbatim", "widthratio", "with", "endwith", "static",
  "get_static_prefix", "get_media_prefix", "trans", "translate", "blocktrans", "blocktranslate", "endblocktrans",
  "endblocktranslate", "plural", "language", "endlanguage", "get_current_language", "localize", "endlocalize",
  "timezone", "endtimezone", "in", "not", "and", "or", "is", "as", "only", "from", "by", "on", "off", "silent",
  "reversed", "noop", "context", "count", "asvar",
];

/** Django templates: `{{ }}`, `{% %}` and `{# #}` with Django's tags. */
export const djangoSyntax: TemplateSyntax = {
  name: "django",
  delimiters: twigDelimiters({}),
  keywords: DJANGO_KEYWORDS,
  endPrefix: "end",
};

export const jinjaSyntax: TemplateSyntax = jinjaSyntaxWith();

const SMARTY_KEYWORDS = [
  "if", "elseif", "else", "foreach", "foreachelse", "for", "while", "section", "sectionelse", "include",
  "assign", "capture", "function", "block", "extends", "literal", "nocache", "strip", "as", "to", "step",
  "eq", "ne", "neq", "gt", "lt", "gte", "ge", "lte", "le", "not", "and", "or", "mod", "is", "even", "odd",
];

/**
 * Smarty with the given delimiters (`$smarty->left_delimiter` / `right_delimiter`). Comments,
 * closing tags and `{literal}` follow them: with `[[` `]]` they are `[[* *]]`, `[[/if]]` and
 * `[[literal]]…[[/literal]]`.
 */
export function smartySyntaxWith(left = "{", right = "}"): TemplateSyntax {
  return {
    name: "smarty",
    delimiters: [
      [`${left}*`, `*${right}`, "comment"],
      [`${left}/`, right, "end"],
      [left, right, "block"],
    ],
    keywords: SMARTY_KEYWORDS,
    autoLiteral: true,
    literalBlocks: { literal: `${left}/literal${right}` },
  };
}

export const smartySyntax: TemplateSyntax = smartySyntaxWith();

export const latteSyntax: TemplateSyntax = {
  name: "latte",
  delimiters: [
    ["{*", "*}", "comment"],
    ["{/", "}", "end"],
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
    ["{{/", "}}", "end"],
    ["{{^", "}}", "block"],
    ["{{>", "}}", "block"],
    ["{{", "}}", "expr"],
  ],
  keywords: ["if", "unless", "each", "with", "else", "as", "this", "lookup", "log"],
};

/**
 * Go `text/template` and `html/template` with the given delimiters (`Template.Delims`):
 * actions with `-` trim markers, block comments, `.Field` selectors, `$variables`, pipelines
 * and builtin functions.
 */
export function goTemplateSyntaxWith(left = "{{", right = "}}"): TemplateSyntax {
  return {
    name: "gotemplate",
    delimiters: [
      [`${left}- /*`, `*/ -${right}`, "comment"],
      [`${left}/*`, `*/${right}`, "comment"],
      [left, right, "expr"],
    ],
    keywords: ["if", "else", "end", "range", "with", "define", "template", "block", "break", "continue"],
    builtins: [
      "and", "or", "not", "len", "index", "slice", "print", "printf", "println", "html", "js", "urlquery",
      "call", "eq", "ne", "lt", "le", "gt", "ge",
    ],
    fields: true,
    rawStrings: true,
  };
}

export const goTemplateSyntax: TemplateSyntax = goTemplateSyntaxWith();

/**
 * ERB (Embedded Ruby, Rails views `*.html.erb`): `<%= expr %>` output tags (`<%==`
 * unescaped), `<% code %>` statement tags with `-%>` trim markers, `<%# comments %>`, and
 * Ruby's keywords and common view helpers inside the tags.
 */
export const erbSyntax: TemplateSyntax = {
  name: "erb",
  delimiters: [
    ["<%#", "%>", "comment"],
    ["<%==", "%>", "expr"],
    ["<%=", "%>", "expr"],
    ["<%-", "%>", "block"],
    ["<%", "%>", "block"],
  ],
  keywords: [
    "if", "elsif", "else", "unless", "end", "do", "case", "when", "while", "until", "for", "in", "begin", "rescue",
    "ensure", "yield", "then", "and", "or", "not", "nil", "true", "false", "self", "return", "next", "break", "def",
    "class", "module", "super", "defined?",
  ],
  builtins: [
    "render", "link_to", "form_with", "form_for", "image_tag", "content_tag", "content_for", "t", "l", "raw", "puts",
    "each", "map", "button_to", "javascript_include_tag", "stylesheet_link_tag", "csrf_meta_tags", "turbo_frame_tag",
    "turbo_stream_from", "turbo_stream", "turbo_include_tags", "turbo_refreshes_with", "javascript_importmap_tags", "dom_id",
    "tag", "simple_format", "truncate", "yield", "fields_for", "time_ago_in_words", "number_to_currency",
  ],
};
