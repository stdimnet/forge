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
  | { t: "break" };

export interface MdListItem {
  checked?: boolean;
  blocks: MdBlock[];
}

export type MdBlock =
  | { t: "heading"; level: number; inline: MdInline[] }
  | { t: "paragraph"; inline: MdInline[] }
  /** Text of a tight list item. */
  | { t: "plain"; inline: MdInline[] }
  | { t: "list"; ordered: boolean; start: number; items: MdListItem[] }
  /** `open`: the fence is not closed yet (streaming). */
  | { t: "code"; lang: string; text: string; open: boolean }
  | { t: "quote"; blocks: MdBlock[] }
  | { t: "hr" };

/** `chat`: raw HTML stays literal text. `docs`: tags dropped, URLs and class names marked. */
export type MdFlavor = "chat" | "docs";

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
};
