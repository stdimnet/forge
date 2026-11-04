import { createEffect, createMemo, on, onCleanup, onMount } from "solid-js";
import { Compartment, EditorState } from "@codemirror/state";
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLineGutter,
  drawSelection,
  highlightSpecialChars,
} from "@codemirror/view";
import { defaultKeymap, historyKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching, foldGutter, indentOnInput } from "@codemirror/language";
import { closeBrackets, autocompletion } from "@codemirror/autocomplete";
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";
import { forgeEditorTheme, forgeHighlight } from "./forgeTheme";
import { indentFor, languageFor } from "./languages";
import { ensureFileType, fileTypeOf, isFileTypeResolved, noteContent } from "./fileTypes";
import { extensionsFor } from "./extensions";
import { updateContent, updateCursor, saveFile, type EditorTab } from "@/state/editor";
import { registerView } from "./views";
import { replaceDocument, undoHistory } from "./reload";
import { wordCompletion } from "./wordCompletion";
import { highlightCaretRows } from "./caretRow";
import { docSync, joinDocSync } from "./docSync";
import { formatCompartment, formatExtensions } from "./codeStyle";
import { errorStripe } from "./errorStripe";
import { usageHighlighting } from "./usageHighlight";
import "./fileFormat";
import { getSetting } from "@/state/settings";

/** Caret blink period (Settings | Editor | General | Caret). */
const caretBlink = () => drawSelection({ cursorBlinkRate: getSetting<number>("editor.caret.blinkPeriod", 500) });

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
  const caretCompartment = new Compartment();
  // Highlighting and the file type's editor features (LSP, inspections, structure) change
  // when the type does: a `#!` line typed into a new file, Override File Type.
  const languageCompartment = new Compartment();
  const featureCompartment = new Compartment();

  onMount(() => {
    // Language, indentation and editor features depend on the file type (resolved in Rust);
    // in the desktop app it may still be on its way.
    const path = props.tab.path;
    if (isFileTypeResolved(path, props.tab.content)) create(path);
    else void ensureFileType(path, props.tab.content).then(() => disposed || create(path));
  });

  function create(path: string) {
    noteContent(path, props.tab.content);
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: props.tab.content,
        extensions: [
          // First, so other views of the file have the change before the store hears of it.
          docSync(path),
          lineNumbers(),
          highlightCaretRows,
          highlightActiveLineGutter(),
          highlightSpecialChars(),
          caretCompartment.of(caretBlink()),
          // Rectangular selection and multi-caret clicks: features/carets.
          undoHistory(),
          indentOnInput(),
          formatCompartment.of(formatExtensions(props.tab.format, indentFor(path, props.tab.content))),
          bracketMatching(),
          closeBrackets(),
          autocompletion(),
          wordCompletion,
          highlightSelectionMatches(),
          usageHighlighting(),
          errorStripe(),
          keymap.of([
            { key: "Mod-s", run: () => (void saveFile(path, "explicit"), true) },
            indentWithTab,
            ...defaultKeymap,
            ...historyKeymap,
            ...searchKeymap,
          ]),
          forgeEditorTheme,
          forgeHighlight,
          languageCompartment.of(languageFor(path)),
          featureCompartment.of(extensionsFor(path)),
          // Folding comes last, next to the text, after feature gutters (markers, VCS), as in PHPStorm.
          foldGutter(),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) {
              updateContent(path, u.state.doc.toString());
              if (u.startState.doc.line(1).text !== u.state.doc.line(1).text) noteContent(path, u.state.doc.line(1).text);
            }
            if (u.selectionSet || u.docChanged) {
              const head = u.state.selection.main.head;
              const line = u.state.doc.lineAt(head);
              updateCursor(path, line.number, head - line.from + 1, u.state.selection.ranges.length);
            }
          }),
        ],
      }),
    });
    unregister = registerView(path, view, props.group);
    leaveSync = joinDocSync(path, view);
  }

  // Re-detected or overridden file type: new highlighting, indentation and features. A
  // lookup still on its way (desktop app) keeps the current type instead of plain text.
  let typeKey = "";
  const fileType = createMemo(() => {
    const key = JSON.stringify(fileTypeOf(props.tab.path));
    if (isFileTypeResolved(props.tab.path)) typeKey = key;
    return typeKey;
  });
  createEffect(
    on(
      fileType,
      () => {
        if (!view) return;
        const path = props.tab.path;
        view.dispatch({
          effects: [
            languageCompartment.reconfigure(languageFor(path)),
            featureCompartment.reconfigure(extensionsFor(path)),
            formatCompartment.reconfigure(formatExtensions(props.tab.format, indentFor(path))),
          ],
        });
      },
      { defer: true },
    ),
  );

  // The resolved format (code style, EditorConfig, detected indent) and the caret settings
  // apply to the open editor.
  createEffect(
    on(
      () => props.tab.format,
      (format) => view?.dispatch({ effects: formatCompartment.reconfigure(formatExtensions(format, indentFor(props.tab.path))) }),
      { defer: true },
    ),
  );
  createEffect(
    on(
      () => getSetting<number>("editor.caret.blinkPeriod", 500),
      () => view?.dispatch({ effects: caretCompartment.reconfigure(caretBlink()) }),
      { defer: true },
    ),
  );

  // Content replaced from outside (e.g. reloaded after an AI agent edited the file). Shared
  // tabs (Code With Me) change through their editor binding, never as whole documents.
  createEffect(() => {
    const content = props.tab.content;
    if (view && !props.tab.shared && content !== view.state.doc.toString()) replaceDocument(view, content);
  });

  onCleanup(() => {
    disposed = true;
    unregister?.();
    leaveSync?.();
    view?.destroy();
  });

  return <div class="code-editor" ref={host} />;
}
