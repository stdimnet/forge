/**
 * Reader mode (Settings | Editor | Reader Mode): files you only read (library files under
 * `vendor/` or `node_modules/`, the PHP stubs, read-only files) open with rendered
 * documentation comments, font ligatures and increased line spacing. The "Reader Mode"
 * button at the top right of the editor switches it off and on for that editor.
 */
import { createEffect, createRoot, on } from "solid-js";
import { StateEffect, StateField, type EditorState, type Extension } from "@codemirror/state";
import { EditorView, ViewPlugin, type PluginValue, type ViewUpdate } from "@codemirror/view";
import { BookOpen } from "lucide-solid";
import { iconNode } from "@/features/lsp/util";
import { isLibraryPath } from "@/ipc/codeintel";
import { getSetting } from "@/state/settings";
import { renderedDocsField, setRenderAll } from "./renderedDocs";

/** Library code: `vendor/`, `node_modules/` and `library://` files (PHP stubs). */
export const isLibraryFile = (path: string): boolean => isLibraryPath(path) || /(^|\/)(vendor|node_modules)\//.test(path);

/** Whether Reader mode applies to a file by the settings. */
export function readerModeApplies(path: string, readOnly: boolean): boolean {
  if (!getSetting<boolean>("editor.readerMode.enabled", true)) return false;
  const mode = getSetting<string>("editor.readerMode.applyTo", "both");
  const library = isLibraryFile(path);
  return mode === "libraries" ? library : mode === "readOnly" ? readOnly : library || readOnly;
}

/** Docs are rendered by default in this editor (outside Reader mode: the setting). */
export const renderDocsByDefault = (reader: boolean): boolean =>
  getSetting<boolean>("editor.renderDocComments", false) || (reader && getSetting<boolean>("editor.readerMode.renderDocs", true));

export const setReaderMode = StateEffect.define<boolean>();

interface ReaderState {
  /** Reader mode is available (the button is shown). */
  available: boolean;
  on: boolean;
}

export const readerModeField = StateField.define<ReaderState>({
  create: () => ({ available: false, on: false }),
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setReaderMode)) return { available: true, on: e.value };
    return value;
  },
  provide: (f) =>
    EditorView.editorAttributes.from(f, (v): Record<string, string> => {
      if (!v.on) return {};
      const classes = ["cm-reader-mode"];
      if (getSetting<boolean>("editor.readerMode.ligatures", true)) classes.push("cm-reader-ligatures");
      if (getSetting<boolean>("editor.readerMode.increaseLineSpacing", true)) classes.push("cm-reader-spacing");
      return { class: classes.join(" ") };
    }),
});

/** Switch Reader mode in an editor (and its rendered doc comments). */
export function toggleReaderMode(view: EditorView, on = !view.state.field(readerModeField, false)?.on): void {
  const effects: StateEffect<unknown>[] = [setReaderMode.of(on)];
  if (view.state.field(renderedDocsField, false)) effects.push(setRenderAll.of(renderDocsByDefault(on)));
  view.dispatch({ effects });
}

export const readerModeOn = (state: EditorState): boolean => state.field(readerModeField, false)?.on ?? false;

class ReaderButton implements PluginValue {
  readonly dom: HTMLButtonElement;
  private readonly dispose: () => void;

  constructor(readonly view: EditorView) {
    this.dom = document.createElement("button");
    this.dom.className = "cm-reader-toggle";
    this.dom.append(iconNode(BookOpen, 14), document.createElement("span"));
    this.dom.addEventListener("mousedown", (e) => e.preventDefault());
    this.dom.addEventListener("click", () => toggleReaderMode(view));
    view.dom.appendChild(this.dom);
    this.draw();
    // Settings that change what Reader mode shows apply to open editors.
    this.dispose = createRoot((d) => {
      createEffect(
        on(
          () => [
            getSetting<boolean>("editor.readerMode.ligatures", true),
            getSetting<boolean>("editor.readerMode.increaseLineSpacing", true),
          ],
          () => queueMicrotask(() => readerModeOn(view.state) && view.dispatch({ effects: setReaderMode.of(true) })),
          { defer: true },
        ),
      );
      return d;
    });
  }

  update(u: ViewUpdate): void {
    if (u.startState.field(readerModeField, false) !== u.state.field(readerModeField, false) || u.geometryChanged) this.draw();
  }

  private draw(): void {
    const state = this.view.state.field(readerModeField, false);
    this.dom.style.display = state?.available ? "" : "none";
    this.dom.classList.toggle("on", !!state?.on);
    this.dom.title = state?.on ? "Reader Mode: on. Click to edit the file as plain code" : "Reader Mode";
    (this.dom.lastChild as HTMLElement).textContent = state?.on ? "Reader Mode" : "";
    this.view.requestMeasure({
      key: this,
      read: (v) => {
        const editor = v.dom.getBoundingClientRect();
        const scroller = v.scrollDOM.getBoundingClientRect();
        return { top: scroller.top - editor.top, right: editor.right - scroller.left - v.scrollDOM.clientWidth };
      },
      write: (m) => {
        this.dom.style.top = `${m.top + 6}px`;
        this.dom.style.right = `${m.right + 14}px`;
      },
    });
  }

  destroy(): void {
    this.dispose();
    this.dom.remove();
  }
}

const readerTheme = EditorView.baseTheme({
  "&.cm-reader-spacing .cm-scroller": { lineHeight: "calc(var(--editor-line-height, 1.6) * 1.3)" },
  "&.cm-reader-ligatures .cm-scroller": { fontVariantLigatures: "normal" },
  ".cm-reader-toggle": {
    position: "absolute",
    zIndex: "6",
    display: "inline-flex",
    alignItems: "center",
    gap: "5px",
    height: "22px",
    padding: "0 6px",
    border: "1px solid transparent",
    borderRadius: "var(--radius)",
    background: "var(--bg-editor)",
    color: "var(--fg-muted)",
    font: "var(--fs-small) var(--font-ui)",
    cursor: "pointer",
  },
  ".cm-reader-toggle:hover": { background: "var(--bg-hover)", color: "var(--fg)" },
  ".cm-reader-toggle.on": { color: "var(--accent)", borderColor: "var(--border-strong)" },
});

/** Reader mode for an editor of `path`; it starts on when the settings say so. */
export function readerMode(path: string): Extension {
  return [
    readerModeField.init((state) => {
      const available = readerModeApplies(path, state.readOnly) || isLibraryFile(path) || state.readOnly;
      return { available, on: readerModeApplies(path, state.readOnly) };
    }),
    ViewPlugin.define((view) => new ReaderButton(view)),
    readerTheme,
  ];
}
