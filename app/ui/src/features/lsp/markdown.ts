/**
 * Small, safe markdown renderer for language server documentation.
 *
 * It only ever creates elements with `document.createElement` and sets text with
 * `textContent`; raw HTML in the source is never interpreted (a handful of harmless tags
 * like `<br>` / `<p>` become line breaks, every other tag is dropped). Links are rendered
 * as link-styled text with the target in the tooltip, never as navigable `href`s.
 */
import { highlightCode, tagHighlighter, tags as t } from "@lezer/highlight";
import { phpLanguage } from "@codemirror/lang-php";

const tokenClasses = tagHighlighter([
  { tag: [t.keyword, t.modifier, t.controlKeyword, t.operatorKeyword, t.definitionKeyword], class: "tok-keyword" },
  { tag: [t.string, t.special(t.string)], class: "tok-string" },
  { tag: [t.number, t.bool, t.null, t.atom], class: "tok-number" },
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], class: "tok-comment" },
  { tag: [t.variableName, t.special(t.variableName)], class: "tok-variable" },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], class: "tok-function" },
  { tag: [t.className, t.typeName, t.namespace], class: "tok-type" },
  { tag: [t.propertyName, t.attributeName], class: "tok-property" },
  { tag: [t.operator, t.punctuation, t.bracket], class: "tok-operator" },
]);

let phpProgramParser: ReturnType<typeof phpLanguage.parser.configure> | undefined;

/** Append syntax-highlighted code (PHP; other languages as plain text) to `parent`. */
export function highlightInto(parent: HTMLElement, code: string, lang = "php"): void {
  if (!/^(php|)$/i.test(lang)) {
    parent.append(document.createTextNode(code));
    return;
  }
  phpProgramParser ??= phpLanguage.parser.configure({ top: "Program" });
  const tree = phpProgramParser.parse(code);
  highlightCode(
    code,
    tree,
    tokenClasses,
    (text, classes) => {
      if (!classes) {
        parent.append(document.createTextNode(text));
        return;
      }
      const span = document.createElement("span");
      span.className = classes;
      span.textContent = text;
      parent.append(span);
    },
    () => parent.append(document.createTextNode("\n")),
  );
}

const LINKABLE = /https?:\/\/[^\s<>()]+[^\s<>().,;:!?]|\\?[A-Za-z_][A-Za-z0-9_]*(?:\\[A-Za-z_][A-Za-z0-9_]*)+/g;

/** Text with URLs and fully qualified class names rendered link-like. */
function appendText(parent: Node, text: string): void {
  let last = 0;
  for (const m of text.matchAll(LINKABLE)) {
    if (m.index! > last) parent.appendChild(document.createTextNode(text.slice(last, m.index)));
    const span = document.createElement("span");
    const url = /^https?:/.test(m[0]);
    span.className = url ? "md-link" : "md-fqcn";
    span.textContent = m[0];
    span.title = url ? m[0] : m[0].replace(/^\\/, "");
    parent.appendChild(span);
    last = m.index! + m[0].length;
  }
  if (last < text.length) parent.appendChild(document.createTextNode(text.slice(last)));
}

const BREAK_TAG = /^<\s*(br|p|\/p)\s*\/?>/i;
const ANY_TAG = /^<\/?[a-zA-Z][^>]*>/;

/** Render inline markdown (code, bold, italics, strike, links, escapes) into `parent`. */
export function renderInline(parent: Node, src: string): void {
  let buf = "";
  const flush = () => {
    if (buf) appendText(parent, buf);
    buf = "";
  };
  let i = 0;
  while (i < src.length) {
    const ch = src[i];
    const rest = src.slice(i);
    // Escapes; a backslash before a letter is a PHP namespace separator and kept as is.
    if (ch === "\\" && /[\\`*_{}[\]()#+\-.!~<>|$]/.test(src[i + 1] ?? "")) {
      buf += src[i + 1];
      i += 2;
      continue;
    }
    if (ch === "`") {
      const ticks = /^`+/.exec(rest)![0];
      const end = src.indexOf(ticks, i + ticks.length);
      if (end > 0) {
        flush();
        const code = document.createElement("code");
        appendText(code, src.slice(i + ticks.length, end).trim());
        parent.appendChild(code);
        i = end + ticks.length;
        continue;
      }
    }
    const strong = /^(\*\*|__)(?=\S)([\s\S]*?\S)\1/.exec(rest);
    if (strong) {
      flush();
      const el = document.createElement("strong");
      renderInline(el, strong[2]);
      parent.appendChild(el);
      i += strong[0].length;
      continue;
    }
    const strike = /^~~(?=\S)([\s\S]*?\S)~~/.exec(rest);
    if (strike) {
      flush();
      const el = document.createElement("del");
      renderInline(el, strike[1]);
      parent.appendChild(el);
      i += strike[0].length;
      continue;
    }
    const em = /^([*_])(?=\S)([\s\S]*?\S)\1(?![A-Za-z0-9])/.exec(rest);
    if (em && !(ch === "_" && /[A-Za-z0-9]/.test(src[i - 1] ?? ""))) {
      flush();
      const el = document.createElement("em");
      renderInline(el, em[2]);
      parent.appendChild(el);
      i += em[0].length;
      continue;
    }
    const link = /^\[([^\]]*)\]\(([^)\s]*)(?:\s+"[^"]*")?\)/.exec(rest);
    if (link) {
      flush();
      const el = document.createElement("span");
      el.className = "md-link";
      el.title = link[2];
      renderInline(el, link[1] || link[2]);
      parent.appendChild(el);
      i += link[0].length;
      continue;
    }
    if (ch === "<") {
      const br = BREAK_TAG.exec(rest);
      if (br) {
        flush();
        parent.appendChild(document.createElement("br"));
        i += br[0].length;
        continue;
      }
      const tag = ANY_TAG.exec(rest);
      if (tag) {
        i += tag[0].length;
        continue;
      }
    }
    if (ch === "&") {
      const ent = /^&(amp|lt|gt|quot|#39|nbsp);/.exec(rest);
      if (ent) {
        buf += { amp: "&", lt: "<", gt: ">", quot: '"', "#39": "'", nbsp: " " }[ent[1]];
        i += ent[0].length;
        continue;
      }
    }
    buf += ch;
    i++;
  }
  flush();
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  return e;
}

/** Render a markdown document into a fragment. */
export function renderMarkdown(md: string): DocumentFragment {
  const frag = document.createDocumentFragment();
  const lines = md.replace(/\r\n?/g, "\n").split("\n");
  let para: string[] = [];
  let list: HTMLElement | null = null;

  const flushPara = () => {
    if (!para.length) return;
    const p = el("p");
    para.forEach((line, idx) => {
      const hardBreak = / {2,}$|\\$/.test(line);
      renderInline(p, line.replace(/( {2,}|\\)$/, "").trim());
      if (idx < para.length - 1) p.appendChild(hardBreak ? el("br") : document.createTextNode(" "));
    });
    frag.appendChild(p);
    para = [];
  };
  const endList = () => {
    list = null;
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const fence = /^\s*(```+|~~~+)\s*([\w-]*)/.exec(line);
    if (fence) {
      flushPara();
      endList();
      const body: string[] = [];
      for (i++; i < lines.length && !lines[i].trim().startsWith(fence[1]); i++) body.push(lines[i]);
      const pre = el("pre", "md-code");
      const code = el("code");
      highlightInto(code, body.join("\n"), fence[2] || "php");
      pre.appendChild(code);
      frag.appendChild(pre);
      continue;
    }
    if (!line.trim()) {
      flushPara();
      endList();
      continue;
    }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
      flushPara();
      endList();
      frag.appendChild(el("hr"));
      continue;
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      flushPara();
      endList();
      const h = el("div", `md-h md-h${heading[1].length}`);
      renderInline(h, heading[2].replace(/\s+#+\s*$/, ""));
      frag.appendChild(h);
      continue;
    }
    const item = /^\s*([-*+]|\d+[.)])\s+(.*)$/.exec(line);
    if (item) {
      flushPara();
      const ordered = /\d/.test(item[1]);
      if (!list || (list.tagName === "OL") !== ordered) {
        list = el(ordered ? "ol" : "ul");
        frag.appendChild(list);
      }
      const li = el("li");
      renderInline(li, item[2]);
      list.appendChild(li);
      continue;
    }
    const quote = /^\s*>\s?(.*)$/.exec(line);
    if (quote) {
      flushPara();
      endList();
      const q = el("blockquote");
      renderInline(q, quote[1]);
      frag.appendChild(q);
      continue;
    }
    if (list && /^\s{2,}\S/.test(line)) {
      // Continuation of the previous list item.
      const li = list.lastElementChild!;
      li.appendChild(document.createTextNode(" "));
      renderInline(li, line.trim());
      continue;
    }
    endList();
    para.push(line);
  }
  flushPara();
  return frag;
}
