/**
 * PHPStorm-style documentation popup for a language server hover: owner FQCN on top, the
 * signature in mono with syntax colors, the description, then "Params:" / "Returns:" /
 * "Throws:" sections. The hover Markdown is split and parsed by Rust (`markdownApi.hover`).
 */
import { Box } from "lucide-solid";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { markdownApi, type HoverDoc } from "@/ipc/markdown";
import { highlightInto } from "./highlightCode";
import { appendBlocks, appendInlines } from "./MarkdownDoc";
import { severityIcon } from "./severity";
import { h, iconNode } from "./util";

/** PHPStorm-like problem lines shown above the documentation. */
function problemsBlock(diags: EditorDiagnostic[]): HTMLElement {
  const block = h("div", "lsp-hover-problems");
  for (const d of diags) {
    const row = h(
      "div",
      `lsp-hover-problem sev-${d.severity}`,
      iconNode(severityIcon[d.severity], 14, "sev-icon"),
      h("span", "lsp-hover-problem-text", d.message),
    );
    block.append(row);
  }
  return block;
}

/** The parts of hover Markdown, or null when there is none. */
export async function hoverDocOf(markdown: string | null | undefined): Promise<HoverDoc | null> {
  if (!markdown?.trim()) return null;
  const plain: HoverDoc = { body: [{ t: "paragraph", inline: [{ t: "text", text: markdown }] }], sections: [] };
  return markdownApi.hover(markdown).catch(() => plain);
}

/** Build the hover popup DOM. Either part may be missing; `lang` colors the signature. */
export function buildHoverDom(doc: HoverDoc | null, diags: EditorDiagnostic[] = [], lang = "php"): HTMLElement {
  const root = h("div", "lsp-hover");
  if (diags.length) root.append(problemsBlock(diags));
  if (!doc) {
    root.classList.add("problems-only");
    return root;
  }
  const docEl = h("div", "lsp-hover-doc");

  if (doc.definition) {
    const def = h("div", "lsp-hover-definition", iconNode(Box, 13, "lsp-hover-definition-icon"));
    const name = doc.definition.includes("\\") ? `\\${doc.definition.replace(/^\\/, "")}` : doc.definition;
    const link = h("span", "md-fqcn", name);
    def.append(link);
    docEl.append(def);
  }
  if (doc.signature) {
    const sig = h("pre", "lsp-hover-signature");
    if (doc.deprecated !== undefined) sig.classList.add("deprecated");
    highlightInto(sig, doc.signature, lang);
    docEl.append(sig);
  }
  if (doc.body.length) {
    const body = h("div", "lsp-hover-body md");
    appendBlocks(body, doc.body);
    docEl.append(body);
  }
  if (doc.deprecated !== undefined || doc.sections.length) {
    const table = h("div", "lsp-hover-sections");
    if (doc.deprecated !== undefined) {
      const dd = h("div", "lsp-hover-section-value deprecated-note");
      if (doc.deprecated.length) appendInlines(dd, doc.deprecated);
      else dd.append("This element is deprecated.");
      table.append(h("div", "lsp-hover-section-title", "Deprecated:"), dd);
    }
    for (const s of doc.sections) {
      const values = h("div", "lsp-hover-section-value");
      for (const entry of s.entries) {
        const p = h("div", "lsp-hover-entry");
        appendInlines(p, entry);
        values.append(p);
      }
      table.append(h("div", "lsp-hover-section-title", s.title), values);
    }
    docEl.append(table);
  }
  root.append(docEl);
  return root;
}
