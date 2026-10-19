/** Helpers shared by the LSP editor extensions. */
import type { Component } from "solid-js";
import { createRoot } from "solid-js";
import type { Text } from "@codemirror/state";
import type { LspPosition } from "@/ipc/lsp";

/** CodeMirror offset -> LSP position (UTF-16 based, like CodeMirror). */
export function toLsp(doc: Text, pos: number): LspPosition {
  const line = doc.lineAt(Math.max(0, Math.min(pos, doc.length)));
  return { line: line.number - 1, character: pos - line.from };
}

/** LSP position -> CodeMirror offset, clamped to the document. */
export function fromLsp(doc: Text, p: LspPosition): number {
  if (p.line < 0) return 0;
  if (p.line >= doc.lines) return doc.length;
  const line = doc.line(p.line + 1);
  return Math.min(line.from + Math.max(0, p.character), line.to);
}

type IconComponent = Component<{ size?: number; "stroke-width"?: number; class?: string }>;

/** Render a lucide icon to a DOM node for use outside Solid components (tooltips, widgets). */
export function iconNode(Icon: IconComponent, size = 14, className?: string): Element {
  return createRoot((dispose) => {
    const node = Icon({ size, "stroke-width": 2, class: className }) as unknown as Element;
    dispose();
    return node;
  });
}

export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  ...children: (Node | string | null | undefined | false)[]
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  for (const c of children) if (c) e.append(c);
  return e;
}

/** Last path segment. */
export const baseName = (path: string) => path.split("/").pop() ?? path;
