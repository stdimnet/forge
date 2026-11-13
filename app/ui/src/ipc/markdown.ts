/**
 * Markdown as a safe AST computed in Rust (`crates/forge-markdown`, `commands/markdown.rs`).
 * The UI renders it with Solid components; raw HTML never reaches the DOM.
 */
import { invoke } from "./index";

export type MdInline =
  | { t: "text"; text: string }
  /** `parts`: documentation only, the code with URLs and class names marked. */
  | { t: "code"; text: string; parts?: MdInline[] }
  | { t: "strong"; children: MdInline[] }
  | { t: "em"; children: MdInline[] }
  | { t: "del"; children: MdInline[] }
  | { t: "link"; href: string; children: MdInline[] }
  | { t: "url"; text: string }
  | { t: "fqcn"; text: string }
  | { t: "break" }
  /** Preview only: `src` as written (relative paths resolve against the document). */
  | { t: "image"; src: string; alt: string; title?: string; width?: string; height?: string }
  /** Preview only: a footnote reference, numbered in the order of first use. */
  | { t: "footnoteRef"; label: string; index: number };

export interface MdListItem {
  checked?: boolean;
  blocks: MdBlock[];
}

export type MdBlock =
  /** `id`: GitHub-style anchor (preview only). */
  | { t: "heading"; level: number; inline: MdInline[]; id?: string }
  | { t: "paragraph"; inline: MdInline[] }
  /** Text of a tight list item. */
  | { t: "plain"; inline: MdInline[] }
  | { t: "list"; ordered: boolean; start: number; items: MdListItem[] }
  /** `open`: the fence is not closed yet (streaming). */
  | { t: "code"; lang: string; text: string; open: boolean }
  /** `kind`: a GitHub alert (`note`, `tip`, `important`, `warning`, `caution`), preview only. */
  | { t: "quote"; blocks: MdBlock[]; kind?: string }
  | { t: "hr" }
  /** Preview only. `align` per column: `left`, `center`, `right` or empty. */
  | { t: "table"; align: string[]; head: MdInline[][]; rows: MdInline[][][] };

/**
 * `chat`: raw HTML stays literal text. `docs`: tags dropped, URLs and class names marked.
 * `preview`: GitHub-flavored, raw HTML sanitized into the AST.
 */
export type MdFlavor = "chat" | "docs" | "preview";

/** 0-based source lines of a top-level block. */
export interface MdLineRange {
  start: number;
  end: number;
}

export interface MdFrontMatter {
  format: "yaml" | "toml";
  entries: { key: string; value: string }[];
  /** 0-based line of the closing fence. */
  endLine: number;
}

export interface MdFootnote {
  label: string;
  index: number;
  blocks: MdBlock[];
  line: number;
}

/** A Markdown file for the preview (`forge_markdown::parse_document`). */
export interface MdDocument {
  blocks: MdBlock[];
  /** Source lines of `blocks[i]`, for synchronized scrolling. */
  lines: MdLineRange[];
  frontMatter?: MdFrontMatter;
  footnotes?: MdFootnote[];
}

export interface HoverSection {
  title: string;
  entries: MdInline[][];
}

/** A language server hover split into the parts of the documentation popup. */
export interface HoverDoc {
  definition?: string;
  signature?: string;
  body: MdBlock[];
  sections: HoverSection[];
  deprecated?: MdInline[];
}

export const markdownApi = {
  parse: (text: string, flavor: MdFlavor = "chat") => invoke<MdBlock[]>("markdown_parse", { text, flavor }),
  hover: (markdown: string) => invoke<HoverDoc>("markdown_hover", { markdown }),
  /** A Markdown file for the preview. */
  document: (text: string) => invoke<MdDocument>("markdown_document", { text }),
  /** 0-based line of the heading with this anchor (`#install`), null when there is none. */
  anchorLine: (text: string, anchor: string) => invoke<number | null>("markdown_anchor_line", { text, anchor }),
};
