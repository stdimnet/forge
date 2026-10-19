/**
 * Small, safe Markdown parser for assistant messages. Produces a tree that is rendered
 * with Solid components (never innerHTML), so model output cannot inject markup.
 * Supports headings, paragraphs, (nested-flat) lists incl. task items, block quotes,
 * fenced code, rules, and inline code / bold / italic / links.
 */

export type Inline =
  | { t: "text"; text: string }
  | { t: "code"; text: string }
  | { t: "strong"; text: string }
  | { t: "em"; text: string }
  | { t: "link"; text: string; href: string };

export interface ListItem {
  inline: Inline[];
  checked?: boolean;
  depth: number;
}

export type MdBlock =
  | { t: "heading"; level: number; inline: Inline[] }
  | { t: "paragraph"; inline: Inline[] }
  | { t: "list"; ordered: boolean; start: number; items: ListItem[] }
  | { t: "code"; lang: string; text: string; open: boolean }
  | { t: "quote"; inline: Inline[] }
  | { t: "hr" };

const INLINE_RE = /(`+)([^`]|[^`][\s\S]*?[^`])\1(?!`)|\*\*([^*]+)\*\*|__([^_]+)__|\*([^*\s][^*]*)\*|(?<![\w])_([^_\s][^_]*)_(?![\w])|\[([^\]]+)\]\(([^)\s]+)\)/g;

export function parseInline(src: string): Inline[] {
  const out: Inline[] = [];
  let last = 0;
  for (const m of src.matchAll(INLINE_RE)) {
    const i = m.index ?? 0;
    if (i > last) out.push({ t: "text", text: src.slice(last, i) });
    if (m[1]) out.push({ t: "code", text: m[2].trim() || m[2] });
    else if (m[3] ?? m[4]) out.push({ t: "strong", text: (m[3] ?? m[4])! });
    else if (m[5] ?? m[6]) out.push({ t: "em", text: (m[5] ?? m[6])! });
    else if (m[7]) out.push({ t: "link", text: m[7], href: m[8] });
    last = i + m[0].length;
  }
  if (last < src.length) out.push({ t: "text", text: src.slice(last) });
  return out;
}

const LIST_RE = /^(\s*)([-*+]|(\d+)[.)])\s+(.*)$/;

export function parseMarkdown(src: string): MdBlock[] {
  const lines = src.replace(/\r\n/g, "\n").split("\n");
  const blocks: MdBlock[] = [];
  let para: string[] = [];
  const flush = () => {
    if (para.length) blocks.push({ t: "paragraph", inline: parseInline(para.join(" ")) });
    para = [];
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const fence = line.match(/^\s*(```+|~~~+)\s*([\w+#.-]*)/);
    if (fence) {
      flush();
      const marker = fence[1];
      const body: string[] = [];
      let open = true;
      for (i++; i < lines.length; i++) {
        if (lines[i].trim().startsWith(marker)) {
          open = false;
          break;
        }
        body.push(lines[i]);
      }
      blocks.push({ t: "code", lang: fence[2].toLowerCase(), text: body.join("\n"), open });
      continue;
    }
    if (!line.trim()) {
      flush();
      continue;
    }
    const heading = line.match(/^(#{1,6})\s+(.*?)\s*#*$/);
    if (heading) {
      flush();
      blocks.push({ t: "heading", level: heading[1].length, inline: parseInline(heading[2]) });
      continue;
    }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
      flush();
      blocks.push({ t: "hr" });
      continue;
    }
    if (line.startsWith(">")) {
      flush();
      const body: string[] = [];
      for (; i < lines.length && lines[i].startsWith(">"); i++) body.push(lines[i].replace(/^>\s?/, ""));
      i--;
      blocks.push({ t: "quote", inline: parseInline(body.join(" ")) });
      continue;
    }
    const li = line.match(LIST_RE);
    if (li) {
      flush();
      const ordered = !!li[3];
      const list: MdBlock & { t: "list" } = { t: "list", ordered, start: ordered ? Number(li[3]) : 1, items: [] };
      for (; i < lines.length; i++) {
        const m = lines[i].match(LIST_RE);
        if (m) {
          let text = m[4];
          let checked: boolean | undefined;
          const task = text.match(/^\[([ xX])\]\s+(.*)$/);
          if (task) {
            checked = task[1] !== " ";
            text = task[2];
          }
          list.items.push({ inline: parseInline(text), checked, depth: Math.min(3, Math.floor(m[1].length / 2)) });
        } else if (lines[i].trim() && /^\s{2,}/.test(lines[i]) && list.items.length) {
          // Continuation line of the previous item.
          const prev = list.items[list.items.length - 1];
          prev.inline.push({ t: "text", text: " " }, ...parseInline(lines[i].trim()));
        } else break;
      }
      i--;
      blocks.push(list);
      continue;
    }
    para.push(line.trim());
  }
  flush();
  return blocks;
}
