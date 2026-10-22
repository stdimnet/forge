/**
 * Synchronized scrolling between the Markdown editor and its preview. Every top-level block
 * of the preview carries its source lines (`data-start` / `data-end`, from Rust); positions
 * in between are interpolated, so long blocks and blank lines scroll smoothly.
 */
import type { EditorView } from "@codemirror/view";

interface Anchor {
  start: number;
  /** One past the last line. */
  end: number;
  top: number;
  bottom: number;
}

function anchorsOf(container: HTMLElement): Anchor[] {
  const box = container.getBoundingClientRect();
  const out: Anchor[] = [];
  for (const el of container.querySelectorAll<HTMLElement>("[data-start]")) {
    const start = Number(el.dataset.start);
    const end = Number(el.dataset.end ?? start);
    if (!Number.isFinite(start)) continue;
    const r = el.getBoundingClientRect();
    out.push({ start, end: Math.max(start, end) + 1, top: r.top - box.top + container.scrollTop, bottom: r.bottom - box.top + container.scrollTop });
  }
  return out.sort((a, b) => a.start - b.start);
}

/** Preview offset of a (fractional, 0-based) source line. */
export function offsetOfLine(anchors: Anchor[], line: number): number {
  if (!anchors.length) return 0;
  let i = -1;
  while (i + 1 < anchors.length && anchors[i + 1].start <= line) i++;
  if (i < 0) return anchors[0].start > 0 ? (line / anchors[0].start) * anchors[0].top : anchors[0].top;
  const a = anchors[i];
  if (line < a.end) return a.top + ((line - a.start) / (a.end - a.start)) * (a.bottom - a.top);
  const next = anchors[i + 1];
  if (!next) return a.bottom;
  const gap = Math.max(1, next.start - a.end);
  return a.bottom + ((line - a.end) / gap) * (next.top - a.bottom);
}

/** Source line (fractional, 0-based) at a preview offset. */
export function lineOfOffset(anchors: Anchor[], y: number): number {
  if (!anchors.length) return 0;
  let i = -1;
  while (i + 1 < anchors.length && anchors[i + 1].top <= y) i++;
  if (i < 0) return anchors[0].top > 0 ? (y / anchors[0].top) * anchors[0].start : anchors[0].start;
  const a = anchors[i];
  if (y < a.bottom) return a.start + ((y - a.top) / Math.max(1, a.bottom - a.top)) * (a.end - a.start);
  const next = anchors[i + 1];
  if (!next) return a.end;
  return a.end + ((y - a.bottom) / Math.max(1, next.top - a.bottom)) * (next.start - a.end);
}

/** Top padding of the editor's document inside its scroller. */
const editorPadding = (view: EditorView) => view.documentTop - view.scrollDOM.getBoundingClientRect().top + view.scrollDOM.scrollTop;

/** Scroll the preview to the editor's first visible line. */
export function editorToPreview(view: EditorView, container: HTMLElement, blocks: number): void {
  const scroller = view.scrollDOM;
  if (scroller.scrollTop <= 1) {
    container.scrollTop = 0;
    return;
  }
  if (scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 2) {
    container.scrollTop = container.scrollHeight;
    return;
  }
  if (!blocks) return;
  const height = Math.max(0, scroller.scrollTop - editorPadding(view));
  const block = view.lineBlockAtHeight(height);
  const line = view.state.doc.lineAt(block.from).number - 1;
  const frac = block.height > 0 ? Math.min(1, Math.max(0, (height - block.top) / block.height)) : 0;
  container.scrollTop = offsetOfLine(anchorsOf(container), line + frac);
}

/** Scroll the editor to the source of the preview's first visible block. */
export function previewToEditor(view: EditorView, container: HTMLElement): void {
  const scroller = view.scrollDOM;
  if (container.scrollTop <= 1) {
    scroller.scrollTop = 0;
    return;
  }
  if (container.scrollTop + container.clientHeight >= container.scrollHeight - 2) {
    scroller.scrollTop = scroller.scrollHeight;
    return;
  }
  const pos = lineOfOffset(anchorsOf(container), container.scrollTop);
  const doc = view.state.doc;
  const line = Math.min(doc.lines, Math.max(1, Math.floor(pos) + 1));
  const block = view.lineBlockAt(doc.line(line).from);
  scroller.scrollTop = block.top + (pos - Math.floor(pos)) * block.height + editorPadding(view);
}
