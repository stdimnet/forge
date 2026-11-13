/**
 * A small, safe Markdown renderer for merge request descriptions and comments: the input is
 * HTML-escaped first, then a subset of CommonMark/GFM is turned into tags (headings, lists,
 * task lists, code, quotes, emphasis, links, `#12`/`!12` references and @mentions).
 */

const esc = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#39;");

/** Inline markup of an already escaped line. */
function inline(text: string, refPrefix: string): string {
  const codes: string[] = [];
  let s = text.replace(/`([^`]+)`/g, (_m, c: string) => {
    codes.push(`<code>${c}</code>`);
    return `\u0000${codes.length - 1}\u0000`;
  });
  // Links are set aside like code spans, so emphasis, mentions and references do not run
  // inside their URLs (`https://x.dev/a_b_c` keeps its underscores).
  const links: string[] = [];
  const link = (html: string) => {
    links.push(html);
    return `\u0001${links.length - 1}\u0001`;
  };
  s = s
    .replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g, (_m, text: string, url: string) => link(`<a href="${url}" target="_blank" rel="noreferrer">${text}</a>`))
    .replace(/(^|[\s(])(https?:\/\/[^\s<]+[^\s<.,:;"')\]])/g, (_m, pre: string, url: string) => pre + link(`<a href="${url}" target="_blank" rel="noreferrer">${url}</a>`))
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/__([^_]+)__/g, "<strong>$1</strong>")
    .replace(/(^|[^*\w])\*([^*\s][^*]*)\*/g, "$1<em>$2</em>")
    .replace(/(^|[^_\w])_([^_\s][^_]*)_(?!\w)/g, "$1<em>$2</em>")
    .replace(/~~([^~]+)~~/g, "<del>$1</del>")
    .replace(/(^|[\s(])@([A-Za-z0-9_.-]+)/g, '$1<span class="md-mention">@$2</span>')
    .replace(/(^|[\s(])([#!])(\d+)\b/g, (_m, pre: string, sign: string, n: string) =>
      sign === "!" && refPrefix !== "!" ? `${pre}!${n}` : `${pre}<span class="md-ref" data-ref="${n}">${sign}${n}</span>`,
    );
  return s.replace(/\u0001(\d+)\u0001/g, (_m, i: string) => links[Number(i)]).replace(/\u0000(\d+)\u0000/g, (_m, i: string) => codes[Number(i)]);
}

export function renderMarkdown(src: string, refPrefix = "#"): string {
  const lines = src.replace(/\r\n?/g, "\n").split("\n");
  const out: string[] = [];
  let para: string[] = [];
  let list: { tag: "ul" | "ol"; items: string[] } | null = null;
  let quote: string[] = [];

  const flushPara = () => {
    if (para.length) out.push(`<p>${para.map((l) => inline(esc(l), refPrefix)).join("<br>")}</p>`);
    para = [];
  };
  const flushList = () => {
    if (list) out.push(`<${list.tag}>${list.items.map((i) => `<li>${i}</li>`).join("")}</${list.tag}>`);
    list = null;
  };
  const flushQuote = () => {
    if (quote.length) out.push(`<blockquote>${renderMarkdown(quote.join("\n"), refPrefix)}</blockquote>`);
    quote = [];
  };
  const flushAll = () => {
    flushPara();
    flushList();
    flushQuote();
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const fence = /^\s*(```|~~~)\s*([\w+-]*)\s*$/.exec(line);
    if (fence) {
      flushAll();
      const code: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trim().startsWith(fence[1])) code.push(lines[i++]);
      out.push(`<pre class="md-code"><code>${esc(code.join("\n"))}</code></pre>`);
      continue;
    }
    if (/^\s*>/.test(line)) {
      flushPara();
      flushList();
      quote.push(line.replace(/^\s*> ?/, ""));
      continue;
    }
    flushQuote();
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      flushAll();
      const level = Math.min(heading[1].length + 1, 6);
      out.push(`<h${level}>${inline(esc(heading[2]), refPrefix)}</h${level}>`);
      continue;
    }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
      flushAll();
      out.push("<hr>");
      continue;
    }
    const item = /^\s*([-*+]|\d+[.)])\s+(.*)$/.exec(line);
    if (item) {
      flushPara();
      const tag = /\d/.test(item[1]) ? "ol" : "ul";
      if (list && list.tag !== tag) flushList();
      if (!list) list = { tag, items: [] };
      const task = /^\[([ xX])\]\s+(.*)$/.exec(item[2]);
      list.items.push(
        task
          ? `<input type="checkbox" disabled${task[1] === " " ? "" : " checked"}> ${inline(esc(task[2]), refPrefix)}`
          : inline(esc(item[2]), refPrefix),
      );
      continue;
    }
    if (!line.trim()) {
      flushAll();
      continue;
    }
    if (list && /^\s{2,}\S/.test(line)) {
      list.items[list.items.length - 1] += ` ${inline(esc(line.trim()), refPrefix)}`;
      continue;
    }
    flushList();
    para.push(line);
  }
  flushAll();
  return out.join("\n");
}
