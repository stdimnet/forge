/**
 * Turns a language server hover (markdown as sent by Intelephense, phpactor & co.) into a
 * PHPStorm-style documentation popup: owner FQCN on top, the signature in mono with syntax
 * colors, the description, then "Params:" / "Returns:" / "Throws:" sections.
 */
import { Box } from "lucide-solid";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { highlightInto, renderInline, renderMarkdown } from "./markdown";
import { severityIcon } from "./severity";
import { h, iconNode } from "./util";

export interface HoverSection {
  title: string;
  entries: string[];
}

export interface HoverDoc {
  /** Owner of the symbol, e.g. `\App\Models\User`. */
  definition?: string;
  signature?: string;
  /** Remaining markdown. */
  body: string;
  sections: HoverSection[];
  /** Set (possibly empty) when the symbol is deprecated. */
  deprecated?: string;
}

const SECTION_TITLES: Record<string, string> = {
  param: "Params:",
  return: "Returns:",
  returns: "Returns:",
  throws: "Throws:",
  see: "See also:",
  link: "See also:",
  since: "Since:",
  var: "Type:",
  example: "Example:",
};

const TAG_LINE = /^\s*(?:[_*]{1,2})?@([a-zA-Z][\w-]*?)(?:[_*]{1,2})?(?:\s+(.*))?$/;
const HEADER_LINE = /^\s*(\*\*|__)(.+?)\1\s*$/;

function cleanSignature(code: string): string {
  return code
    .replace(/^\s*<\?php\s*/, "")
    .replace(/\s*\{\s*\}\s*$/, "")
    .trim();
}

/** Split hover markdown into the parts shown in the popup. */
export function parseHover(markdown: string): HoverDoc {
  const doc: HoverDoc = { body: "", sections: [] };
  const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
  const body: string[] = [];
  const sections = new Map<string, HoverSection>();
  let seenContent = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const fence = /^\s*(```+)\s*([\w-]*)\s*$/.exec(line);
    if (fence && doc.signature === undefined && /^(php|)$/i.test(fence[2])) {
      const code: string[] = [];
      for (i++; i < lines.length && !lines[i].trim().startsWith(fence[1]); i++) code.push(lines[i]);
      doc.signature = cleanSignature(code.join("\n"));
      seenContent = true;
      continue;
    }
    if (fence) {
      // Other code blocks stay in the body untouched.
      body.push(line);
      for (i++; i < lines.length; i++) {
        body.push(lines[i]);
        if (lines[i].trim().startsWith(fence[1])) break;
      }
      continue;
    }
    const header = HEADER_LINE.exec(line);
    if (header && !seenContent && doc.definition === undefined) {
      const name = header[2].replace(/\\\\/g, "\\");
      // Members show their class, classes their namespace, global functions nothing.
      if (name.includes("::")) doc.definition = name.slice(0, name.lastIndexOf("::"));
      else if (name.includes("\\")) doc.definition = name.slice(0, name.lastIndexOf("\\"));
      else doc.definition = "";
      seenContent = true;
      continue;
    }
    const tag = TAG_LINE.exec(line);
    if (tag) {
      const name = tag[1].toLowerCase();
      const text = (tag[2] ?? "").replace(/( {2,}|\\)$/, "").trim();
      if (name === "deprecated") {
        doc.deprecated = text;
        continue;
      }
      const title = SECTION_TITLES[name] ?? `${name[0].toUpperCase()}${name.slice(1)}:`;
      const section = sections.get(title) ?? { title, entries: [] };
      section.entries.push(text);
      sections.set(title, section);
      continue;
    }
    if (line.trim()) seenContent = true;
    body.push(line);
  }

  doc.body = body
    .join("\n")
    .replace(/^\s*(---|\*\*\*|___)\s*$/gm, "\n")
    .trim();
  const order = ["Type:", "Params:", "Returns:", "Throws:", "Since:", "See also:"];
  doc.sections = [...sections.values()].sort(
    (a, b) => (order.indexOf(a.title) + 1 || 99) - (order.indexOf(b.title) + 1 || 99),
  );
  return doc;
}

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

/** Build the hover popup DOM. Either part may be missing. */
export function buildHoverDom(markdown: string | null | undefined, diags: EditorDiagnostic[] = []): HTMLElement {
  const root = h("div", "lsp-hover");
  if (diags.length) root.append(problemsBlock(diags));
  if (!markdown?.trim()) {
    root.classList.add("problems-only");
    return root;
  }
  const doc = parseHover(markdown);
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
    highlightInto(sig, doc.signature, "php");
    docEl.append(sig);
  }
  if (doc.body) {
    const body = h("div", "lsp-hover-body md");
    body.append(renderMarkdown(doc.body));
    docEl.append(body);
  }
  if (doc.deprecated !== undefined || doc.sections.length) {
    const table = h("div", "lsp-hover-sections");
    if (doc.deprecated !== undefined) {
      const dd = h("div", "lsp-hover-section-value deprecated-note");
      renderInline(dd, doc.deprecated || "This element is deprecated.");
      table.append(h("div", "lsp-hover-section-title", "Deprecated:"), dd);
    }
    for (const s of doc.sections) {
      const values = h("div", "lsp-hover-section-value");
      for (const entry of s.entries) {
        const p = h("div", "lsp-hover-entry");
        renderInline(p, entry);
        values.append(p);
      }
      table.append(h("div", "lsp-hover-section-title", s.title), values);
    }
    docEl.append(table);
  }
  root.append(docEl);
  return root;
}
