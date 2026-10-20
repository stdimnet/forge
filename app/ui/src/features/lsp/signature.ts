/**
 * Parameter Info (Ctrl+P): a popup above the caret listing the parameters of the call
 * being typed, the current one in bold. Opens automatically after `(` and updates on `,`.
 */
import { StateEffect, StateField, type Extension } from "@codemirror/state";
import { EditorView, keymap, showTooltip, ViewPlugin, type Tooltip, type ViewUpdate } from "@codemirror/view";
import { lspApi, type EditorSignatureHelp } from "@/ipc/lsp";
import { renderMarkdown } from "./markdown";
import { syncNow } from "./sync";
import { h, toLsp } from "./util";

const setSignature = StateEffect.define<{ pos: number; help: EditorSignatureHelp } | null>();

const signatureField = StateField.define<Tooltip | null>({
  create: () => null,
  update(tip, tr) {
    for (const e of tr.effects) {
      if (e.is(setSignature)) {
        if (!e.value) return null;
        const { pos, help } = e.value;
        return { pos, above: true, strictSide: true, create: () => ({ dom: buildSignatureDom(help) }) };
      }
    }
    if (tip && tr.docChanged) return { ...tip, pos: tr.changes.mapPos(tip.pos) };
    return tip;
  },
  provide: (f) => showTooltip.from(f),
});

/** The parameter list part of a signature label: `f(int $a, int $b): int` -> offsets of `int $a, int $b`. */
function paramsSpan(label: string): [number, number] {
  const open = label.indexOf("(");
  if (open < 0) return [0, label.length];
  let depth = 0;
  for (let i = open; i < label.length; i++) {
    if (label[i] === "(") depth++;
    else if (label[i] === ")" && --depth === 0) return [open + 1, i];
  }
  return [open + 1, label.length];
}

export function buildSignatureDom(help: EditorSignatureHelp): HTMLElement {
  const root = h("div", "cm-lsp-signature");
  help.signatures.forEach((sig, idx) => {
    const active = sig.activeParameter ?? help.activeParameter;
    const row = h("div", "cm-lsp-signature-row");
    if (idx === help.activeSignature) row.classList.add("active");
    const [start, end] = paramsSpan(sig.label);
    if (!sig.parameters.length || start >= end) {
      row.append(h("span", "cm-lsp-signature-none", "<no parameters>"));
    } else {
      let cursor = start;
      sig.parameters.forEach((p, i) => {
        const [ps, pe] = p.label;
        if (ps > cursor) row.append(sig.label.slice(cursor, ps));
        const param = h("span", "cm-lsp-param", sig.label.slice(ps, pe));
        if (i === active && idx === help.activeSignature) param.classList.add("current");
        row.append(param);
        cursor = pe;
      });
      if (cursor < end) row.append(sig.label.slice(cursor, end));
    }
    root.append(row);
  });
  const current = help.signatures[help.activeSignature];
  const paramDoc = current?.parameters[current.activeParameter ?? help.activeParameter]?.documentation;
  if (paramDoc) {
    const doc = h("div", "cm-lsp-signature-doc md");
    doc.append(renderMarkdown(paramDoc));
    root.append(doc);
  }
  return root;
}

async function requestSignature(path: string, view: EditorView, explicit: boolean): Promise<boolean> {
  const pos = view.state.selection.main.head;
  await syncNow(path);
  const help = await lspApi.signatureHelp(path, toLsp(view.state.doc, pos)).catch(() => null);
  if (view.state.selection.main.head !== pos && !explicit) return false;
  view.dispatch({ effects: setSignature.of(help?.signatures.length ? { pos, help } : null) });
  return !!help;
}

export function parameterInfo(path: string, view: EditorView): boolean {
  void requestSignature(path, view, true);
  return true;
}

export function lspSignatureHelp(path: string): Extension {
  return [
    signatureField,
    keymap.of([
      { key: "Mod-p", run: (view) => parameterInfo(path, view) },
      {
        key: "Escape",
        run: (view) => {
          if (!view.state.field(signatureField)) return false;
          view.dispatch({ effects: setSignature.of(null) });
          return true;
        },
      },
    ]),
    ViewPlugin.define((view) => ({
      update(u: ViewUpdate) {
        const open = !!u.state.field(signatureField);
        let typed = "";
        u.transactions.forEach((tr) => {
          if (tr.isUserEvent("input.type")) tr.changes.iterChanges((_a, _b, _c, _d, text) => (typed += text.toString()));
        });
        const head = u.state.selection.main.head;
        const completedCall =
          u.transactions.some((tr) => tr.isUserEvent("input.complete")) && u.state.sliceDoc(head - 1, head + 1) === "()";
        if (typed.endsWith("(") || completedCall || (open && typed.endsWith(","))) {
          setTimeout(() => void requestSignature(path, view, false));
        } else if (open && (typed.endsWith(")") || (u.selectionSet && !u.docChanged))) {
          // Leaving the call closes the popup; moving inside it refreshes the highlight.
          setTimeout(() => void requestSignature(path, view, false));
        }
      },
    })),
  ];
}
