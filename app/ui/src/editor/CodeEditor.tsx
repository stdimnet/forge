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
import { ensureFileType, isFileTypeResolved } from "./fileTypes";
import { extensionsFor } from "./extensions";
import { updateContent, updateCursor, saveFile, type EditorTab } from "@/state/editor";
import { registerView } from "./views";
import { replaceDocument, undoHistory } from "./reload";
import { wordCompletion } from "./wordCompletion";
import { docSync, joinDocSync } from "./docSync";

/**
 * One CodeMirror instance per tab of an editor group; kept alive while the tab exists. Views
 * of the same file in other groups edit the same document (`docSync`).
 */
export function CodeEditor(props: { tab: EditorTab; group?: number }) {
  let host!: HTMLDivElement;
  let view: EditorView | undefined;
  let unregister: (() => void) | undefined;
  let leaveSync: (() => void) | undefined;

  let disposed = false;

  onMount(() => {
    // Language, indentation and editor features depend on the file type (resolved in Rust);
    // in the desktop app it may still be on its way.
    const path = props.tab.path;
    if (isFileTypeResolved(path, props.tab.content)) create(path);
    else void ensureFileType(path, props.tab.content).then(() => disposed || create(path));
  });

  function create(path: string) {
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: props.tab.content,
        extensions: [
          // First, so other views of the file have the change before the store hears of it.
          docSync(path),
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
    unregister = registerView(path, view, props.group);
    leaveSync = joinDocSync(path, view);
  }

  // Content replaced from outside (e.g. reloaded after an AI agent edited the file).
  createEffect(() => {
    const content = props.tab.content;
    if (view && content !== view.state.doc.toString()) replaceDocument(view, content);
  });

  onCleanup(() => {
    disposed = true;
    unregister?.();
    leaveSync?.();
    view?.destroy();
  });

  return <div class="code-editor" ref={host} />;
}
