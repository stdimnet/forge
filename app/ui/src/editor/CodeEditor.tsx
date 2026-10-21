import { createEffect, on, onCleanup, onMount } from "solid-js";
import { Compartment, EditorState } from "@codemirror/state";
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLine,
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
import { ensureFileType, isFileTypeResolved } from "./fileTypes";
import { extensionsFor } from "./extensions";
import { updateContent, updateCursor, saveFile, type EditorTab } from "@/state/editor";
import { registerView } from "./views";
import { replaceDocument, undoHistory } from "./reload";
import { wordCompletion } from "./wordCompletion";
import { docSync, joinDocSync } from "./docSync";
import { formatCompartment, formatExtensions } from "./codeStyle";
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
              updateCursor(path, line.number, head - line.from + 1, u.state.selection.ranges.length);
            }
          }),
        ],
      }),
    });
    unregister = registerView(path, view, props.group);
    leaveSync = joinDocSync(path, view);
  }

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
