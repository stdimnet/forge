/**
 * Syntax highlighting for diff lines outside an editor: each side of a hunk (old = context
 * + removed lines, new = context + added lines) is parsed with the file's language and
 * split into colored segments per line, using the editor's token colors.
 */
import { Language, LanguageSupport } from "@codemirror/language";
import { highlightTree } from "@lezer/highlight";
import type { Extension } from "@codemirror/state";
import { languageFor } from "@/editor/languages";
import { isFileTypeResolved } from "@/editor/fileTypes";
import { forgeHighlightStyle } from "@/editor/forgeTheme";

export interface Segment {
  text: string;
  cls?: string;
}

type Parser = Language["parser"];

function findLanguage(ext: Extension): Language | undefined {
  if (ext instanceof LanguageSupport) return ext.language;
  if (ext instanceof Language) return ext;
  if (Array.isArray(ext)) {
    for (const e of ext) {
      const l = findLanguage(e);
      if (l) return l;
    }
  }
  return undefined;
}

const cache = new Map<string, Parser | null>();

/** Parser for a file; PHP files are parsed as code (fragments lack the `<?php` tag). */
export function parserFor(path: string): Parser | null {
  const ext = path.slice(path.lastIndexOf(".") + 1).toLowerCase();
  if (cache.has(ext)) return cache.get(ext)!;
  let parser: Parser | null = null;
  try {
    const lang = findLanguage(languageFor(path));
    if (lang) {
      parser = lang.parser;
      if (lang.name === "php" && "configure" in parser) {
        parser = (parser as unknown as { configure: (c: { top: string }) => Parser }).configure({ top: "Program" });
      }
    }
  } catch {
    parser = null;
  }
  // Until the file type has arrived (desktop app) the answer is provisional; callers in a
  // reactive scope re-run once it lands.
  if (isFileTypeResolved(path)) cache.set(ext, parser);
  return parser;
}

let styleMounted = false;

/** The highlight classes need their CSS even when no editor is open. */
function mountStyle(): void {
  if (styleMounted || typeof document === "undefined") return;
  styleMounted = true;
  const rules = forgeHighlightStyle.module?.getRules();
  if (!rules) return;
  const el = document.createElement("style");
  el.dataset.forge = "review-highlight";
  el.textContent = rules;
  document.head.append(el);
}

/** Split `lines` into highlighted segments (one array per line). */
export function highlightLines(lines: string[], parser: Parser | null): Segment[][] {
  if (!parser || !lines.length) return lines.map((text) => [{ text }]);
  mountStyle();
  const text = lines.join("\n");
  const ranges: { from: number; to: number; cls: string }[] = [];
  try {
    highlightTree(parser.parse(text), forgeHighlightStyle, (from, to, cls) => ranges.push({ from, to, cls }));
  } catch {
    return lines.map((t) => [{ text: t }]);
  }
  const out: Segment[][] = [];
  let offset = 0;
  let r = 0;
  for (const line of lines) {
    const start = offset;
    const end = offset + line.length;
    const segs: Segment[] = [];
    let pos = start;
    while (r < ranges.length && ranges[r].to <= start) r++;
    for (let k = r; k < ranges.length && ranges[k].from < end; k++) {
      const from = Math.max(ranges[k].from, start);
      const to = Math.min(ranges[k].to, end);
      if (from > pos) segs.push({ text: text.slice(pos, from) });
      if (to > from) segs.push({ text: text.slice(from, to), cls: ranges[k].cls });
      pos = Math.max(pos, to);
    }
    if (pos < end) segs.push({ text: text.slice(pos, end) });
    out.push(segs);
    offset = end + 1;
  }
  return out;
}
