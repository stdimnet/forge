/**
 * The code editor of a cell: CodeMirror with Python (or Markdown) highlighting and the
 * editor's theme. Running, mode switches and the other notebook shortcuts come from the
 * keymap (contexts `notebook` / `notebook-command`), never from listeners here.
 */
import { onCleanup, onMount } from "solid-js";
import { EditorState, type Extension } from "@codemirror/state";
import { EditorView, drawSelection, highlightSpecialChars, keymap, placeholder } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching, indentOnInput, indentUnit } from "@codemirror/language";
import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { python } from "@codemirror/lang-python";
import { markdown } from "@codemirror/lang-markdown";
import { forgeEditorTheme, forgeHighlight } from "@/editor/forgeTheme";
import type { CellType } from "@/ipc/jupyter";
import type { NotebookController } from "./store";

const cellTheme = EditorView.theme({
  "&": { background: "transparent" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { overflow: "visible", fontFamily: "var(--font-mono)", lineHeight: "1.55" },
  ".cm-content": { padding: "6px 0" },
  ".cm-line": { padding: "0 10px" },
});

function languageOf(cellType: CellType, language: string): Extension[] {
  if (cellType === "markdown") return [markdown()];
  if (cellType === "raw") return [];
  return language === "python" || language === "" ? [python()] : [];
}

export function CellEditor(props: { ctl: NotebookController; cellId: string; cellType: CellType; source: string; language: string; autofocus?: boolean }) {
  let host!: HTMLDivElement;
  let view: EditorView | undefined;
  onMount(() => {
    const index = () => props.ctl.cellIndex(props.cellId);
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: props.source,
        extensions: [
          highlightSpecialChars(),
          drawSelection(),
          history(),
          indentOnInput(),
          bracketMatching(),
          closeBrackets(),
          indentUnit.of("    "),
          EditorState.tabSize.of(4),
          keymap.of([...closeBracketsKeymap, ...defaultKeymap, ...historyKeymap, indentWithTab]),
          forgeEditorTheme,
          forgeHighlight,
          cellTheme,
          EditorView.lineWrapping,
          placeholder(props.cellType === "markdown" ? "Markdown text" : props.cellType === "raw" ? "Raw text" : ""),
          ...languageOf(props.cellType, props.language),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) props.ctl.setSource(props.cellId, u.state.doc.toString());
            // Clicking into a cell's editor selects the cell in edit mode.
            if (u.focusChanged && u.view.hasFocus && index() >= 0) props.ctl.editorFocused(index());
          }),
        ],
      }),
    });
    props.ctl.views.set(props.cellId, view);
    if (props.autofocus) queueMicrotask(() => view?.focus());
  });
  onCleanup(() => {
    if (view && props.ctl.views.get(props.cellId) === view) props.ctl.views.delete(props.cellId);
    view?.destroy();
  });
  return <div class="nb-input-editor" ref={host} />;
}
