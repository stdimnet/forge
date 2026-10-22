/**
 * Editor side of Emmet: the expand key (Tab, Enter or Space after an abbreviation) and the
 * abbreviation preview popup. Expand keys are CodeMirror editor keys like Tab's
 * indentation; whether a key expands is decided in Rust, which runs in-process as
 * WebAssembly because the key handler must answer at once. Live templates and postfix
 * templates bind the same keys with higher precedence, so they win when they match.
 */
import { Prec, StateField, type Extension } from "@codemirror/state";
import { EditorView, keymap, showTooltip, type Tooltip } from "@codemirror/view";
import { indentUnit } from "@codemirror/language";
import { getSetting } from "@/state/settings";
import { languageIdFor } from "@/editor/languages";
import { emmetApi, type EmmetExpansion } from "@/ipc/emmet";
import { applyExpansion, emmetFamily, EXPAND_KEY, expandRequest, PREVIEW_KEY } from "./model";

type ExpandKey = "tab" | "enter" | "space";

/** Expand the abbreviation before the caret; false when there is none (the key acts as usual). */
export function expandAbbreviation(path: string, view: EditorView): boolean {
  const sel = view.state.selection;
  if (sel.ranges.length > 1 || !sel.main.empty) return false;
  const head = sel.main.head;
  // Cheap check first: an abbreviation ends with a word character, `)`, `]`, `}`, `*`, `!` or `%`.
  if (!/[\w)\]}*!%$]$/.test(view.state.sliceDoc(Math.max(0, head - 1), head))) return false;
  const e = emmetApi.expandSync(expandRequest(path, view));
  if (!e) return false;
  applyExpansion(view, e);
  return true;
}

const expandKey = (path: string, key: ExpandKey) => (view: EditorView) =>
  getSetting<string>(EXPAND_KEY, "tab") === key && expandAbbreviation(path, view);

const MAX_PREVIEW_LINES = 16;

function previewTooltip(e: EmmetExpansion): Tooltip {
  return {
    pos: e.from,
    above: false,
    strictSide: false,
    arrow: false,
    create: () => {
      const dom = document.createElement("div");
      dom.className = "emmet-preview";
      const pre = document.createElement("pre");
      const lines = e.text.split("\n");
      pre.textContent = lines.slice(0, MAX_PREVIEW_LINES).join("\n") + (lines.length > MAX_PREVIEW_LINES ? "\n…" : "");
      dom.append(pre);
      return { dom };
    },
  };
}

/**
 * Settings | Editor | Emmet | HTML | Enable abbreviation preview: the expanded markup below
 * the abbreviation while it is typed; it closes when the caret leaves or the key expands.
 */
function previewField(path: string) {
  return StateField.define<Tooltip | null>({
    create: () => null,
    update(tip, tr) {
      if (!tr.docChanged) return tr.selection ? null : tip;
      if (!tr.isUserEvent("input.type") && !tr.isUserEvent("delete")) return null;
      if (!getSetting<boolean>(PREVIEW_KEY, false)) return null;
      const sel = tr.state.selection;
      if (sel.ranges.length > 1 || !sel.main.empty) return null;
      const request = {
        text: tr.state.doc.toString(),
        offset: sel.main.head,
        languageId: languageIdFor(path),
        options: {
          html: getSetting<boolean>("editor.emmet.html.enabled", true),
          css: false,
          jsx: getSetting<boolean>("editor.emmet.jsx.enabled", true),
          jsxClassAttr: getSetting<string>("editor.emmet.jsx.classAttribute", "className"),
          hrefRecognition: true,
          indent: tr.state.facet(indentUnit),
        },
        preview: true,
      };
      try {
        const e = emmetApi.expandSync(request);
        return e ? previewTooltip(e) : null;
      } catch {
        return null;
      }
    },
    provide: (f) => showTooltip.from(f),
  });
}

export function emmetExtension(path: string): Extension | null {
  const family = emmetFamily(languageIdFor(path));
  if (!family) return null;
  return [
    Prec.high(
      keymap.of([
        { key: "Tab", run: expandKey(path, "tab") },
        { key: "Space", run: expandKey(path, "space") },
        { key: "Enter", run: expandKey(path, "enter") },
      ]),
    ),
    family === "css" ? [] : previewField(path),
  ];
}
