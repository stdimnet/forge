/**
 * Blade: HTML with Blade echoes (`{{ }}`, `{!! !!}`), comments (`{{-- --}}`), directives
 * (`@foreach (...)`), `@php` / `<?php` blocks, components (`<x-alert :type="$t">`,
 * `<livewire:counter>`) and embedded `<script>` / `<style>`. Everything PHP inside is
 * highlighted by a PHP expression tokenizer, the rest as HTML, like PHPStorm's Blade
 * language with PHP and HTML injections.
 */
import type { StreamParser, StringStream } from "@codemirror/language";
import { javascript as jsMode } from "@codemirror/legacy-modes/mode/javascript";
import { css as cssMode } from "@codemirror/legacy-modes/mode/css";

export interface BladeDirective {
  name: string;
  /** Takes `(...)` arguments. */
  args?: boolean;
  /** Closing directive inserted by completion (`endforeach`). */
  end?: string;
  description: string;
}

const d = (name: string, description: string, args = false, end?: string): BladeDirective => ({ name, description, args, end });

/** Built-in directives of Laravel 12 (and Livewire's common ones). */
export const BLADE_DIRECTIVES: BladeDirective[] = [
  d("if", "Conditional block", true, "endif"),
  d("elseif", "Else-if branch", true),
  d("else", "Else branch"),
  d("endif", "End of @if"),
  d("unless", "Inverse conditional", true, "endunless"),
  d("endunless", "End of @unless"),
  d("isset", "Block when a variable is set", true, "endisset"),
  d("endisset", "End of @isset"),
  d("empty", "Block when a variable is empty", true, "endempty"),
  d("endempty", "End of @empty"),
  d("auth", "Block for authenticated users", false, "endauth"),
  d("endauth", "End of @auth"),
  d("guest", "Block for guests", false, "endguest"),
  d("endguest", "End of @guest"),
  d("production", "Block in production", false, "endproduction"),
  d("endproduction", "End of @production"),
  d("env", "Block for an environment", true, "endenv"),
  d("endenv", "End of @env"),
  d("hasSection", "Block when a section has content", true, "endif"),
  d("sectionMissing", "Block when a section is missing", true, "endif"),
  d("session", "Block when a session value exists", true, "endsession"),
  d("endsession", "End of @session"),
  d("context", "Block when a context value exists", true, "endcontext"),
  d("endcontext", "End of @context"),
  d("switch", "Switch statement", true, "endswitch"),
  d("case", "Switch case", true),
  d("break", "Break out of a loop or switch"),
  d("default", "Default switch case"),
  d("endswitch", "End of @switch"),
  d("for", "For loop", true, "endfor"),
  d("endfor", "End of @for"),
  d("foreach", "Foreach loop", true, "endforeach"),
  d("endforeach", "End of @foreach"),
  d("forelse", "Foreach loop with an @empty branch", true, "endforelse"),
  d("endforelse", "End of @forelse"),
  d("while", "While loop", true, "endwhile"),
  d("endwhile", "End of @while"),
  d("continue", "Skip to the next iteration"),
  d("include", "Include a view", true),
  d("includeIf", "Include a view if it exists", true),
  d("includeWhen", "Include a view when a condition is true", true),
  d("includeUnless", "Include a view unless a condition is true", true),
  d("includeFirst", "Include the first existing view", true),
  d("includeIsolated", "Include a view without parent data", true),
  d("each", "Render a view for each item", true),
  d("extends", "Extend a layout", true),
  d("section", "Define a section", true, "endsection"),
  d("endsection", "End of @section"),
  d("show", "End a section and yield it"),
  d("yield", "Display a section", true),
  d("parent", "Parent section content"),
  d("stop", "End of @section"),
  d("append", "End a section and append it"),
  d("overwrite", "End a section and overwrite it"),
  d("props", "Component properties", true),
  d("aware", "Properties from a parent component", true),
  d("component", "Render a component", true, "endcomponent"),
  d("endcomponent", "End of @component"),
  d("componentFirst", "Render the first existing component", true, "endcomponentFirst"),
  d("endcomponentFirst", "End of @componentFirst"),
  d("slot", "Component slot", true, "endslot"),
  d("endslot", "End of @slot"),
  d("php", "PHP block", false, "endphp"),
  d("endphp", "End of @php"),
  d("verbatim", "Output without Blade compilation", false, "endverbatim"),
  d("endverbatim", "End of @verbatim"),
  d("csrf", "CSRF token field"),
  d("method", "HTTP method spoofing field", true),
  d("error", "Block for a validation error", true, "enderror"),
  d("enderror", "End of @error"),
  d("can", "Block when authorized", true, "endcan"),
  d("elsecan", "Else-can branch", true),
  d("endcan", "End of @can"),
  d("cannot", "Block when not authorized", true, "endcannot"),
  d("endcannot", "End of @cannot"),
  d("canany", "Block when any ability is authorized", true, "endcanany"),
  d("endcanany", "End of @canany"),
  d("push", "Push to a stack", true, "endpush"),
  d("endpush", "End of @push"),
  d("pushOnce", "Push to a stack once", true, "endPushOnce"),
  d("endPushOnce", "End of @pushOnce"),
  d("pushIf", "Push to a stack if a condition is true", true, "endPushIf"),
  d("endPushIf", "End of @pushIf"),
  d("prepend", "Prepend to a stack", true, "endprepend"),
  d("endprepend", "End of @prepend"),
  d("prependOnce", "Prepend to a stack once", true, "endPrependOnce"),
  d("endPrependOnce", "End of @prependOnce"),
  d("stack", "Render a stack", true),
  d("once", "Render once per rendering cycle", false, "endonce"),
  d("endonce", "End of @once"),
  d("fragment", "Fragment for partial rendering", true, "endfragment"),
  d("endfragment", "End of @fragment"),
  d("lang", "Translate a key", true),
  d("choice", "Translate with pluralization", true),
  d("json", "Encode a value as JSON", true),
  d("js", "Encode a value for JavaScript", true),
  d("class", "Conditional CSS classes", true),
  d("style", "Conditional inline styles", true),
  d("checked", "Conditional checked attribute", true),
  d("selected", "Conditional selected attribute", true),
  d("disabled", "Conditional disabled attribute", true),
  d("readonly", "Conditional readonly attribute", true),
  d("required", "Conditional required attribute", true),
  d("inject", "Inject a service", true),
  d("use", "Import a class", true),
  d("dd", "Dump and die", true),
  d("dump", "Dump variables", true),
  d("vite", "Vite assets", true),
  d("viteReactRefresh", "Vite React refresh script"),
  d("livewire", "Render a Livewire component", true),
  d("livewireStyles", "Livewire styles"),
  d("livewireScripts", "Livewire scripts"),
  d("persist", "Persist an element across navigations", true, "endpersist"),
  d("endpersist", "End of @persist"),
  d("teleport", "Teleport content", true, "endteleport"),
  d("endteleport", "End of @teleport"),
];

const BUILTIN = new Set(BLADE_DIRECTIVES.map((x) => x.name));

const PHP_KEYWORDS = new Set([
  "abstract", "and", "array", "as", "break", "callable", "case", "catch", "class", "clone", "const", "continue",
  "declare", "default", "do", "echo", "else", "elseif", "empty", "enddeclare", "endfor", "endforeach", "endif",
  "endswitch", "endwhile", "enum", "extends", "final", "finally", "fn", "for", "foreach", "function", "global",
  "goto", "if", "implements", "include", "include_once", "instanceof", "insteadof", "interface", "isset", "list",
  "match", "namespace", "new", "or", "print", "private", "protected", "public", "readonly", "require",
  "require_once", "return", "static", "switch", "throw", "trait", "try", "unset", "use", "var", "while", "xor",
  "yield", "self", "parent",
]);

type Mode = "html" | "echo" | "rawEcho" | "comment" | "args" | "php" | "phpTag" | "verbatim";

interface BladeState {
  mode: Mode;
  /** Where to return after an echo / directive inside an attribute value. */
  returnToAttr: boolean;
  /** Parenthesis depth inside directive arguments. */
  depth: number;
  /** Open PHP string quote (multi-line strings in @php blocks). */
  phpString: string | null;
  phpComment: boolean;
  /** The previous PHP token was `->` / `?->` or `::`. */
  afterArrow: boolean;
  afterScope: boolean;
  /** A directive name was just read; `(` may follow (after spaces). */
  pendingArgs: boolean;
  html: "text" | "tag" | "attrValue" | "comment";
  quote: string | null;
  tagName: boolean;
  /** The current attribute is bound (`:type="$x"`) so its value is PHP. */
  phpAttr: boolean;
  /** Inside <script> or <style>. */
  embedded: "script" | "style" | null;
  embeddedState: unknown;
  /** Tag name of the tag being read (to start <script>/<style> after `>`). */
  currentTag: string;
}

export interface BladeOptions {
  /** Extra directive names (without `@`) to highlight, e.g. from `Blade::directive`. */
  customDirectives?: () => string[];
}

function phpToken(stream: StringStream, state: BladeState): string | null {
  if (state.phpComment) {
    if (stream.skipTo("*/")) {
      stream.match("*/");
      state.phpComment = false;
    } else stream.skipToEnd();
    return "comment";
  }
  if (state.phpString) {
    const q = state.phpString;
    let escaped = false;
    let c: string | void;
    while ((c = stream.next()) != null) {
      if (c === q && !escaped) {
        state.phpString = null;
        break;
      }
      escaped = !escaped && c === "\\";
    }
    return "string";
  }
  if (stream.eatSpace()) return null;
  if (stream.match("/*")) {
    state.phpComment = true;
    return phpToken(stream, state) ?? "comment";
  }
  if (stream.match("//") || (stream.peek() === "#" && !stream.string.startsWith("#[", stream.pos) && stream.match("#"))) {
    // A line comment ends at the line end, or at the closer of the surrounding Blade tag.
    const rest = stream.string.slice(stream.pos);
    const close = state.mode === "echo" ? rest.indexOf("}}") : state.mode === "rawEcho" ? rest.indexOf("!!}") : state.mode === "phpTag" ? rest.indexOf("?>") : -1;
    if (close >= 0) stream.pos += close;
    else stream.skipToEnd();
    return "comment";
  }
  const ch = stream.peek()!;
  const arrow = state.afterArrow;
  const scope = state.afterScope;
  state.afterArrow = false;
  state.afterScope = false;
  if (ch === "'" || ch === '"') {
    stream.next();
    state.phpString = ch;
    return phpToken(stream, state) ?? "string";
  }
  if (stream.match(/^\$this\b/)) return "variableName.special";
  if (stream.match(/^\$[A-Za-z_\x80-￿][\w\x80-￿]*/)) return "variableName";
  if (stream.match(/^0[xX][0-9a-fA-F_]+|^\d[\d_]*(\.\d+)?([eE][-+]?\d+)?/)) return "number";
  if (stream.match(/^\??->/)) {
    state.afterArrow = true;
    return "operator";
  }
  if (stream.match("::")) {
    state.afterScope = true;
    return "operator";
  }
  if (stream.match(/^\\?[A-Za-z_\x80-￿][\w\x80-￿]*(\\[A-Za-z_\x80-￿][\w\x80-￿]*)*/)) {
    const word = stream.current();
    const call = /^\s*\(/.test(stream.string.slice(stream.pos));
    if (arrow) return call ? "propertyName.function" : "propertyName";
    if (scope) {
      if (word === "class") return "keyword";
      if (call) return "propertyName.function";
      return /^[A-Z0-9_]+$/.test(word) ? "variableName.constant" : "propertyName";
    }
    const lower = word.toLowerCase();
    if (lower === "true" || lower === "false" || lower === "null") return "atom";
    if (PHP_KEYWORDS.has(lower) && !word.includes("\\")) return "keyword";
    if (call && !/^[A-Z]/.test(word.replace(/^\\/, "")) ) return "variableName.function";
    if (word.includes("\\") || /^[A-Z]/.test(word)) {
      if (/^[A-Z0-9_]+$/.test(word) && !call && !/^\s*::/.test(stream.string.slice(stream.pos))) return "variableName.constant";
      return "className";
    }
    return call ? "variableName.function" : "variableName";
  }
  if (stream.match(/^(=>|===|!==|==|!=|<=>|<=|>=|&&|\|\||\?\?=?|\+\+|--|\*\*|\.=|\+=|-=|\*=|\/=|[-+*/%=<>!?:.&|^~@])/)) return "operator";
  if (stream.match(/^[()[\]{}]/)) return "bracket";
  stream.next();
  return "punctuation";
}

const OPENERS = /\{\{--|\{\{|\{!!|@\{\{|@|<\?php|<\?=/;

export function bladeParser(options: BladeOptions = {}): StreamParser<BladeState> {
  const isDirective = (name: string) => BUILTIN.has(name) || (options.customDirectives?.() ?? []).includes(name);
  const js = jsMode as StreamParser<unknown>;
  const css = cssMode as StreamParser<unknown>;

  /** `@name` at the stream position, when it is a directive. */
  function directive(stream: StringStream, state: BladeState): string | null {
    const before = stream.pos > 0 ? stream.string[stream.pos - 1] : "";
    if (/[\w@.]/.test(before)) return null;
    const m = stream.match(/^@([A-Za-z_]\w*)/, false) as RegExpMatchArray | null;
    if (!m) return null;
    const name = m[1];
    // Longest known prefix wins for `@endif` vs unknown words (`@endifx` is text).
    if (!isDirective(name)) return null;
    stream.pos += m[0].length;
    if (name === "php" && !/^\s*\(/.test(stream.string.slice(stream.pos))) {
      state.mode = "php";
    } else if (name === "verbatim") {
      state.mode = "verbatim";
    } else {
      state.pendingArgs = true;
    }
    return "keyword";
  }

  /** Blade constructs that can start anywhere in HTML (text, attribute values). */
  function bladeOpen(stream: StringStream, state: BladeState): string | null {
    if (stream.match("{{--")) {
      if (stream.skipTo("--}}")) stream.match("--}}");
      else {
        stream.skipToEnd();
        state.mode = "comment";
      }
      return "comment";
    }
    if (stream.match("@{{")) return null; // escaped echo, rendered literally
    if (stream.match("{{")) {
      state.mode = "echo";
      return "processingInstruction";
    }
    if (stream.match("{!!")) {
      state.mode = "rawEcho";
      return "processingInstruction";
    }
    if (stream.match(/^<\?(php\b|=)/)) {
      state.mode = "phpTag";
      return "processingInstruction";
    }
    if (stream.peek() === "@") {
      if (stream.match("@@")) return null;
      return directive(stream, state);
    }
    return null;
  }

  function leaveBlade(state: BladeState) {
    state.mode = "html";
    state.afterArrow = false;
    state.afterScope = false;
    if (state.returnToAttr) {
      state.returnToAttr = false;
      state.html = "attrValue";
    }
  }

  /** Content of <script>/<style>, stopping before Blade constructs and the closing tag. */
  function embeddedToken(stream: StringStream, state: BladeState): string | null {
    const closeTag = state.embedded === "script" ? /<\/script\s*>/i : /<\/style\s*>/i;
    const rest = stream.string.slice(stream.pos);
    const close = rest.search(closeTag);
    if (close === 0) {
      state.embedded = null;
      state.html = "tag";
      state.tagName = true;
      stream.match("</");
      return "angleBracket";
    }
    const open = rest.search(OPENERS);
    let end = rest.length;
    if (close > 0) end = Math.min(end, close);
    if (open > 0) end = Math.min(end, open);
    if (open === 0) {
      const start = stream.pos;
      const style = bladeOpen(stream, state);
      if (stream.pos > start) return style;
      stream.next();
      return null;
    }
    const inner = state.embedded === "script" ? js : css;
    const full = stream.string;
    stream.string = full.slice(0, stream.pos + end);
    const start = stream.pos;
    let style: string | null = null;
    try {
      style = (inner.token(stream, state.embeddedState) as string | null) ?? null;
    } finally {
      stream.string = full;
    }
    if (stream.pos === start) stream.next();
    return style;
  }

  function html(stream: StringStream, state: BladeState): string | null {
    if (state.html === "comment") {
      if (stream.skipTo("-->")) {
        stream.match("-->");
        state.html = "text";
      } else stream.skipToEnd();
      return "comment";
    }
    if (state.html === "attrValue") {
      if (state.phpAttr) {
        if (stream.peek() === state.quote) {
          stream.next();
          state.html = "tag";
          state.quote = null;
          state.phpAttr = false;
          return "attributeValue";
        }
        // PHP until the closing quote (strings inside use the other quote).
        const full = stream.string;
        const end = full.indexOf(state.quote!, stream.pos);
        if (end >= 0) stream.string = full.slice(0, end);
        try {
          const start = stream.pos;
          const style = phpToken(stream, state);
          if (stream.pos === start) stream.next();
          return style;
        } finally {
          stream.string = full;
        }
      }
      if (stream.string.startsWith("{{", stream.pos) || stream.string.startsWith("{!!", stream.pos) || (stream.peek() === "@" && /^@[A-Za-z]/.test(stream.string.slice(stream.pos)))) {
        state.returnToAttr = true;
        state.html = "text";
        const style = bladeOpen(stream, state);
        if (style) return style;
        state.returnToAttr = false;
        state.html = "attrValue";
        stream.next();
        return "attributeValue";
      }
      while (!stream.eol()) {
        const rest = stream.string.slice(stream.pos);
        if (rest.startsWith("{{") || rest.startsWith("{!!") || /^@[A-Za-z]/.test(rest)) {
          if (stream.current()) return "attributeValue";
        }
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
        const tag = state.currentTag.toLowerCase();
        if (stream.current() === ">" && (tag === "script" || tag === "style")) {
          state.embedded = tag;
          state.embeddedState = (tag === "script" ? js : css).startState?.(2) ?? {};
        }
        state.currentTag = "";
        return "angleBracket";
      }
      if (state.tagName && stream.match(/^[\w:.-]+/)) {
        state.tagName = false;
        const name = stream.current();
        state.currentTag = name;
        return /^(x-|x:|livewire:|flux:)/.test(name) ? "className" : "tagName";
      }
      // Blade inside the tag (`<div @class([...]) {{ $attributes }}>`).
      if (stream.string.startsWith("{{", stream.pos) || stream.string.startsWith("{!!", stream.pos)) {
        state.returnToAttr = false;
        const style = bladeOpen(stream, state);
        if (style) {
          state.html = "tag";
          return style;
        }
      }
      if (stream.peek() === "@") {
        const style = directive(stream, state);
        if (style) {
          state.html = "tag";
          return style;
        }
      }
      if (stream.match(/^[^\s=>/"'{]+/)) {
        // `:message="$msg"` and `::class` bind PHP (`::` escapes to a literal `:`).
        const name = stream.current();
        state.phpAttr = name.startsWith(":") && !name.startsWith("::");
        return "attributeName";
      }
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
      state.phpAttr = false;
      return "angleBracket";
    }
    if (stream.match(/^&\w+;/)) return "character";
    stream.next();
    while (!stream.eol()) {
      const rest = stream.string.slice(stream.pos);
      if (rest[0] === "<" || rest[0] === "&" || rest.startsWith("{{") || rest.startsWith("{!!") || rest[0] === "@") break;
      stream.next();
    }
    return null;
  }

  return {
    name: "blade",
    startState: () => ({
      mode: "html",
      returnToAttr: false,
      depth: 0,
      phpString: null,
      phpComment: false,
      afterArrow: false,
      afterScope: false,
      pendingArgs: false,
      html: "text",
      quote: null,
      tagName: false,
      phpAttr: false,
      embedded: null,
      embeddedState: null,
      currentTag: "",
    }),
    copyState: (s) => ({
      ...s,
      embeddedState:
        s.embedded && s.embeddedState
          ? ((s.embedded === "script" ? js : css).copyState?.(s.embeddedState) ?? { ...(s.embeddedState as object) })
          : null,
    }),
    token(stream, state) {
      switch (state.mode) {
        case "comment": {
          if (stream.skipTo("--}}")) {
            stream.match("--}}");
            leaveBlade(state);
          } else stream.skipToEnd();
          return "comment";
        }
        case "echo":
        case "rawEcho": {
          const close = state.mode === "echo" ? "}}" : "!!}";
          if (!state.phpString && !state.phpComment && stream.match(close)) {
            leaveBlade(state);
            return "processingInstruction";
          }
          // Stop PHP tokens at the closer.
          const full = stream.string;
          const end = state.phpString ? -1 : full.indexOf(close, stream.pos);
          if (end >= 0) stream.string = full.slice(0, end);
          try {
            const start = stream.pos;
            const style = phpToken(stream, state);
            if (stream.pos === start) stream.next();
            return style;
          } finally {
            stream.string = full;
          }
        }
        case "phpTag": {
          if (!state.phpString && !state.phpComment && stream.match("?>")) {
            leaveBlade(state);
            return "processingInstruction";
          }
          const full = stream.string;
          const end = state.phpString || state.phpComment ? -1 : full.indexOf("?>", stream.pos);
          if (end >= 0) stream.string = full.slice(0, end);
          try {
            const start = stream.pos;
            const style = phpToken(stream, state);
            if (stream.pos === start) stream.next();
            return style;
          } finally {
            stream.string = full;
          }
        }
        case "php": {
          if (!state.phpString && !state.phpComment && stream.match(/^@endphp\b/)) {
            leaveBlade(state);
            return "keyword";
          }
          const full = stream.string;
          const end = state.phpString || state.phpComment ? -1 : full.indexOf("@endphp", stream.pos);
          if (end >= 0) stream.string = full.slice(0, end);
          try {
            const start = stream.pos;
            const style = phpToken(stream, state);
            if (stream.pos === start) stream.next();
            return style;
          } finally {
            stream.string = full;
          }
        }
        case "verbatim": {
          if (stream.match(/^@endverbatim\b/)) {
            leaveBlade(state);
            return "keyword";
          }
          const i = stream.string.indexOf("@endverbatim", stream.pos);
          if (i === stream.pos) return null;
          if (i < 0) stream.skipToEnd();
          else stream.pos = i;
          return null;
        }
        case "args": {
          if (!state.phpString && !state.phpComment) {
            if (stream.peek() === "(") {
              stream.next();
              state.depth++;
              return "bracket";
            }
            if (stream.peek() === ")") {
              stream.next();
              state.depth--;
              if (state.depth <= 0) {
                state.depth = 0;
                leaveBlade(state);
              }
              return "bracket";
            }
          }
          const start = stream.pos;
          const style = phpToken(stream, state);
          if (stream.pos === start) stream.next();
          return style;
        }
        default:
          break;
      }
      if (state.pendingArgs) {
        if (stream.match(/^\s*\(/, false)) {
          stream.eatSpace();
          stream.next();
          state.pendingArgs = false;
          state.mode = "args";
          state.depth = 1;
          if (state.html === "attrValue") state.returnToAttr = true;
          if (state.html === "tag") state.returnToAttr = false;
          return "bracket";
        }
        state.pendingArgs = false;
        if (state.returnToAttr && state.mode === "html") {
          // A directive without arguments inside an attribute value (`@endif"`).
          state.returnToAttr = false;
          state.html = "attrValue";
        }
      }
      if (state.embedded) return embeddedToken(stream, state);
      if (state.html === "text") {
        const style = bladeOpen(stream, state);
        if (style !== null || stream.pos > stream.start) return style;
      }
      return html(stream, state);
    },
    blankLine(state) {
      state.pendingArgs = false;
    },
    languageData: {
      commentTokens: { block: { open: "{{--", close: "--}}" } },
    },
  };
}
