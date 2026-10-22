/**
 * Live template actions: Insert Live Template (Ctrl+J), Surround With (Ctrl+Alt+T), Surround
 * with Live Template (Ctrl+Alt+J), Unwrap/Remove (Ctrl+Shift+Delete) and Save as Live
 * Template. Rust decides what applies (`live_templates_*`); these only show and apply it.
 */
import type { EditorView } from "@codemirror/view";
import { activeView } from "@/editor/views";
import { editor } from "@/state/editor";
import { showBalloon } from "@/keymap/dispatch";
import { openSettings } from "@/features/settings/dialogState";
import { liveTemplatesApi, type SurroundItem } from "@/ipc/liveTemplates";
import { applyExpansion, expandTemplate, setUnwrapHighlight } from "./editor";
import { docRequest, envFor, PAGE_ID } from "./model";
import { openPopup, type PopupItem } from "./Popup";
import { emmetSurroundItem } from "@/features/emmet/actions";

/** The active file editor, if any. */
function target(): { path: string; view: EditorView } | null {
  const view = activeView();
  const path = editor.active;
  return view && path ? { path, view } : null;
}

export async function insertLiveTemplate(): Promise<void> {
  const t = target();
  if (!t) return;
  const { path, view } = t;
  const result = await liveTemplatesApi.applicable({ ...docRequest(path, view), offset: view.state.selection.main.head });
  if (!result.items.length) {
    showBalloon("No applicable live templates");
    return;
  }
  openPopup(view, {
    title: "Insert Live Template",
    filterable: true,
    filter: result.prefix,
    items: result.items.map((i) => ({
      id: i.id,
      label: i.abbreviation,
      detail: i.description,
      code: true,
      run: () => void expandTemplate(path, view, i.id),
    })),
  });
}

function surroundRequest(path: string, view: EditorView, templatesOnly: boolean) {
  const { from, to } = view.state.selection.main;
  return { ...docRequest(path, view), from, to, env: envFor(view), templatesOnly };
}

export async function surroundWith(templatesOnly = false): Promise<void> {
  const t = target();
  if (!t) return;
  const { path, view } = t;
  const request = surroundRequest(path, view, templatesOnly);
  const items = await liveTemplatesApi.surroundItems(request);
  // In markup files Surround with Live Template offers Emmet first, as in PHPStorm.
  const emmet = templatesOnly ? emmetSurroundItem(path, view) : null;
  if (!items.length && !emmet) {
    showBalloon(templatesOnly ? "No applicable live templates" : "Nothing to surround with here");
    return;
  }
  const firstTemplate = items.findIndex((i) => i.kind === "template");
  const toItem = (i: SurroundItem, index: number): PopupItem => ({
    id: i.id,
    label: i.label,
    detail: i.description,
    mnemonic: i.mnemonic,
    code: i.kind === "template",
    section: index === firstTemplate && index > 0 ? "Live templates" : undefined,
    run: async () => {
      const e = await liveTemplatesApi.surround({ ...request, id: i.id });
      if (e) applyExpansion(view, e);
    },
  });
  openPopup(view, {
    title: templatesOnly ? "Surround with Live Template" : "Surround With",
    items: [...(emmet ? [emmet] : []), ...items.map(toItem)],
  });
}

export async function unwrapRemove(): Promise<void> {
  const t = target();
  if (!t) return;
  const { path, view } = t;
  const options = await liveTemplatesApi.unwrap({
    text: view.state.doc.toString(),
    offset: view.state.selection.main.head,
    path,
    languageId: docRequest(path, view).languageId,
  });
  if (!options.length) {
    showBalloon("Nothing to unwrap or remove here");
    return;
  }
  const highlight = (h: Parameters<typeof setUnwrapHighlight.of>[0]) => view.dispatch({ effects: setUnwrapHighlight.of(h) });
  openPopup(view, {
    title: "Choose the statement to unwrap/remove",
    onClose: () => highlight(null),
    items: options.map((o, i) => ({
      id: String(i),
      label: o.label,
      preview: () => highlight({ from: o.from, to: o.to, keep: o.keep }),
      run: () => {
        highlight(null);
        view.dispatch({
          changes: o.edits.map((e) => ({ from: e.from, to: e.to, insert: e.text })),
          selection: { anchor: Math.min(o.from, view.state.doc.length) },
          userEvent: "delete",
          scrollIntoView: true,
        });
        view.focus();
      },
    })),
  });
}

/**
 * The selected text with the indentation of its first line removed from the following lines,
 * since an expansion indents them like the line it is expanded in.
 */
export function dedentSelection(view: EditorView, from: number, to: number): string {
  const line = view.state.doc.lineAt(from);
  const lead = /^[ \t]*/.exec(line.text)![0].length;
  const [first, ...rest] = view.state.sliceDoc(from, to).split("\n");
  const indents = rest.filter((l) => l.trim()).map((l) => /^[ \t]*/.exec(l)![0].length);
  const cut = Math.min(lead, ...indents);
  return [first, ...rest.map((l) => l.slice(Math.min(cut, /^[ \t]*/.exec(l)![0].length)))].join("\n");
}

/** Arguments of the Live Templates page for a new template made from the selection. */
export interface NewTemplateArgs {
  text: string;
  contexts: string[];
}

export async function saveAsLiveTemplate(): Promise<void> {
  const t = target();
  if (!t) return;
  const { path, view } = t;
  const { from, to } = view.state.selection.main;
  if (from === to) {
    showBalloon("Select the text to save as a live template");
    return;
  }
  const result = await liveTemplatesApi.applicable({ ...docRequest(path, view), offset: from });
  // Dollar signs are template syntax; `$$` keeps a literal one.
  const text = dedentSelection(view, from, to).replace(/\$/g, "$$$$");
  openSettings(PAGE_ID, { text, contexts: result.contexts.length ? result.contexts : ["OTHER"] } satisfies NewTemplateArgs);
}
