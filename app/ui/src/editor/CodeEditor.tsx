import { createEffect, onCleanup, onMount } from "solid-js";
import { EditorState } from "@codemirror/state";
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLine,
  highlightActiveLineGutter,
  drawSelection,
  highlightSpecialChars,
  rectangularSelection,
} from "@codemirror/view";
import { defaultKeymap, historyKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching, foldGutter, indentOnInput, indentUnit } from "@codemirror/language";
import { closeBrackets, autocompletion } from "@codemirror/autocomplete";
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";
import { forgeEditorTheme, forgeHighlight } from "./forgeTheme";
import { indentFor, languageFor } from "./languages";
import { extensionsFor } from "./extensions";
import { updateContent, updateCursor, saveFile, type EditorTab } from "@/state/editor";
import { registerView } from "./views";
import { replaceDocument, undoHistory } from "./reload";
import { wordCompletion } from "./wordCompletion";

/** One CodeMirror instance per open tab; kept alive while the tab exists. */
export function CodeEditor(props: { tab: EditorTab }) {
  let host!: HTMLDivElement;
  let view: EditorView | undefined;
  let unregister: (() => void) | undefined;

  onMount(() => {
    const path = props.tab.path;
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: props.tab.content,
        extensions: [
          lineNumbers(),
          highlightActiveLine(),
          highlightActiveLineGutter(),
          highlightSpecialChars(),
          drawSelection(),
          rectangularSelection(),
          undoHistory(),
          indentOnInput(),
          indentUnit.of(indentFor(path, props.tab.content)),
          bracketMatching(),
          closeBrackets(),
          autocompletion(),
          wordCompletion,
          highlightSelectionMatches(),
          keymap.of([
            { key: "Mod-s", run: () => (void saveFile(path), true) },
            indentWithTab,
            ...defaultKeymap,
            ...historyKeymap,
            ...searchKeymap,
          ]),
          forgeEditorTheme,
          forgeHighlight,
          ...languageFor(path, props.tab.content),
          ...extensionsFor(path),
          // Folding comes last, next to the text, after feature gutters (markers, VCS), as in PHPStorm.
          foldGutter(),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) updateContent(path, u.state.doc.toString());
            if (u.selectionSet || u.docChanged) {
              const head = u.state.selection.main.head;
              const line = u.state.doc.lineAt(head);
              updateCursor(path, line.number, head - line.from + 1);
            }
          }),
        ],
      }),
    });
    unregister = registerView(path, view);
  });

  // Content replaced from outside (e.g. reloaded after an AI agent edited the file).
  createEffect(() => {
    const content = props.tab.content;
    if (view && content !== view.state.doc.toString()) replaceDocument(view, content);
  });

  onCleanup(() => {
    unregister?.();
    view?.destroy();
  });

  return <div class="code-editor" ref={host} />;
}
