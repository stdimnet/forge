/**
 * Editor extensions that follow a file's resolved format (`FileFormat` from Rust): the
 * indent unit and tab width, continuation indent of method chains, the right margin and
 * visual guides. They live in a compartment so settings changes apply to open editors.
 */
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import { EditorView, ViewPlugin, type ViewUpdate } from "@codemirror/view";
import { indentService, indentUnit } from "@codemirror/language";
import type { FileFormat } from "@/ipc/text";

export const formatCompartment = new Compartment();

/** A line that continues a method chain: `->call()`, `?->call()` or `.call()`. */
const CHAIN = /^\s*(\?->|->|\.(?=[A-Za-z_$]))/;

/**
 * Continuation indent: a chain line is indented one continuation step deeper than the
 * statement it continues, and further chain lines keep that indent.
 */
function continuationIndent(continuation: number): Extension {
  return [
    indentService.of((context, pos) => {
      const after = context.textAfterPos(pos);
      if (!CHAIN.test(after)) return null;
      let line = context.state.doc.lineAt(pos);
      while (line.number > 1) {
        line = context.state.doc.line(line.number - 1);
        if (line.text.trim()) break;
      }
      if (!line.text.trim()) return null;
      const base = context.lineIndent(line.from);
      return CHAIN.test(line.text) ? base : base + continuation;
    }),
    EditorState.languageData.of(() => [{ indentOnInput: /^\s*(\?->|->)$/ }]),
  ];
}

/** Vertical lines at the right margin and the visual guides. */
function guides(columns: number[]): Extension {
  if (!columns.length) return [];
  return ViewPlugin.fromClass(
    class {
      dom: HTMLDivElement;
      constructor(readonly view: EditorView) {
        this.dom = document.createElement("div");
        this.dom.className = "cm-forge-guides";
        this.dom.setAttribute("aria-hidden", "true");
        for (const c of columns) {
          const line = document.createElement("div");
          line.className = "cm-forge-guide";
          line.dataset.column = String(c);
          this.dom.appendChild(line);
        }
        view.scrollDOM.appendChild(this.dom);
        this.place();
      }
      update(u: ViewUpdate) {
        if (u.geometryChanged || u.viewportChanged) this.place();
      }
      place() {
        this.view.requestMeasure({
          read: (view) => {
            const line = view.contentDOM.querySelector(".cm-line");
            const pad = line ? parseFloat(getComputedStyle(line).paddingLeft) || 0 : 0;
            return {
              left: view.contentDOM.offsetLeft + pad,
              width: view.defaultCharacterWidth,
              height: Math.max(view.contentDOM.offsetHeight, view.scrollDOM.clientHeight),
            };
          },
          write: ({ left, width, height }) => {
            this.dom.style.height = `${height}px`;
            this.dom.querySelectorAll<HTMLElement>(".cm-forge-guide").forEach((el) => {
              el.style.left = `${left + Number(el.dataset.column) * width}px`;
            });
          },
        });
      }
      destroy() {
        this.dom.remove();
      }
    },
  );
}

const guidesTheme = EditorView.baseTheme({
  ".cm-scroller": { position: "relative" },
  ".cm-forge-guides": { position: "absolute", top: 0, left: 0, width: 0, pointerEvents: "none", zIndex: 0 },
  ".cm-forge-guide": { position: "absolute", top: 0, bottom: 0, width: 0, borderLeft: "1px solid var(--border)" },
});

/** Extensions of a resolved format (or the file type's indent until it is resolved). */
export function formatExtensions(format: FileFormat | undefined, fallbackIndent: string): Extension {
  if (!format) return [indentUnit.of(fallbackIndent)];
  const unit = format.indent.useTabs ? "\t" : " ".repeat(Math.max(1, format.indent.size));
  const columns = [...new Set([format.hardWrapAt, ...format.visualGuides])].filter((c) => c > 0).sort((a, b) => a - b);
  return [
    indentUnit.of(unit),
    EditorState.tabSize.of(Math.max(1, format.indent.tabSize)),
    continuationIndent(format.indent.continuation),
    guides(columns),
    guidesTheme,
  ];
}
