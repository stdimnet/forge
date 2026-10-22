import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import "@/mock/settingsMock";
import "@/mock/keymapMock";
import { installKeymap, resetKeymap, setKeymapId } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { history } from "@codemirror/commands";
import type { VimResponse } from "@/ipc/vim";
import { loadVimEngine, vimEngine } from "./engine";
import { refreshVimEditors, setVimHost, vimControllerOf, vimExtension, vimField } from "./extension";

// jsdom has no layout; CodeMirror measures text ranges after updates.
const noRects = () => Object.assign([], { item: () => null }) as unknown as DOMRectList;
Range.prototype.getClientRects ??= noRects;
Range.prototype.getBoundingClientRect ??= () => new DOMRect();

let enabled = true;
const actions: string[] = [];
const copied: string[] = [];
let last: VimResponse | null = null;

beforeAll(async () => {
  await loadVimEngine();
  setVimHost({
    enabled: () => enabled,
    runAction: async (id) => void actions.push(id),
    isActionAvailable: () => true,
    openFile: async () => {},
    reloadRc: async () => {},
    copy: (text) => void copied.push(text),
    status: (_view, res) => (last = res),
  });
  vimEngine.configure(true, []);
});

const views: EditorView[] = [];
afterEach(() => {
  for (const v of views.splice(0)) v.destroy();
  enabled = true;
  vimEngine.configure(true, []);
});

/** An editor with the caret at `|`, attached to the engine. */
async function editor(doc: string): Promise<EditorView> {
  const caret = Math.max(0, doc.indexOf("|"));
  const view = new EditorView({
    parent: document.body,
    state: EditorState.create({
      doc: doc.replace("|", ""),
      selection: EditorSelection.cursor(caret),
      extensions: [history(), EditorState.allowMultipleSelections.of(true), vimExtension()],
    }),
  });
  views.push(view);
  await Promise.resolve();
  return view;
}

function keyEvent(k: string): KeyboardEventInit {
  const named: Record<string, string> = { Esc: "Escape", CR: "Enter", BS: "Backspace" };
  const ctrl = /^C-(.)$/.exec(k);
  if (ctrl) return { key: ctrl[1], code: `Key${ctrl[1].toUpperCase()}`, ctrlKey: true };
  if (named[k]) return { key: named[k], code: named[k] };
  const code = /^[a-z]$/i.test(k) ? `Key${k.toUpperCase()}` : "";
  return { key: k, code, shiftKey: k !== k.toLowerCase() };
}

/** Press keys (`dd`, `ciwx<Esc>`); keys Vim leaves alone are typed like the browser would. */
function type(view: EditorView, keys: string): void {
  for (const m of keys.matchAll(/<([^>]+)>|./gs)) {
    const init = keyEvent(m[1] ?? m[0]);
    const e = new KeyboardEvent("keydown", { ...init, bubbles: true, cancelable: true });
    view.contentDOM.dispatchEvent(e);
    if (!e.defaultPrevented && init.key!.length === 1 && !init.ctrlKey) {
      view.dispatch(view.state.replaceSelection(init.key!), { userEvent: "input.type" });
    }
  }
}

const text = (view: EditorView) => view.state.doc.toString();
const flush = () => new Promise((r) => setTimeout(r, 0));

describe("Vim emulation in the editor", () => {
  it("runs Normal mode commands", async () => {
    const view = await editor("one\n|two\nthree");
    type(view, "dd");
    expect(text(view)).toBe("one\nthree");
    expect(view.state.field(vimField).mode).toBe("normal");
    type(view, "ciwTHREE<Esc>");
    expect(text(view)).toBe("one\nTHREE");
    type(view, "k.");
    expect(text(view)).toBe("THREE\nTHREE");
  });

  it("yanks and pastes in Visual mode", async () => {
    const view = await editor("|foo bar");
    type(view, "vey");
    expect(view.state.field(vimField).mode).toBe("normal");
    type(view, "$p");
    expect(text(view)).toBe("foo barfoo");
    type(view, "0vl");
    expect(view.state.field(vimField).mode).toBe("visual");
    expect(view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to)).toBe("fo");
  });

  it("substitutes, repeats and undoes through the editor history", async () => {
    const view = await editor("|a a\na");
    type(view, ":%s/a/b/g<CR>");
    expect(text(view)).toBe("b b\nb");
    type(view, "u");
    await flush();
    expect(text(view)).toBe("a a\na");
    type(view, "<C-r>");
    await flush();
    expect(text(view)).toBe("b b\nb");
    // The engine follows the undo: the next command sees the current text.
    type(view, "ggx");
    expect(text(view)).toBe(" b\nb");
  });

  it("lets the editor type in Insert mode and repeats the insertion", async () => {
    const view = await editor("|x");
    type(view, "ahi<Esc>");
    expect(text(view)).toBe("xhi");
    expect(view.state.selection.main.head).toBe(2);
    type(view, ".");
    expect(text(view)).toBe("xhihi");
  });

  it("shows the command line while it is typed", async () => {
    const view = await editor("|x");
    type(view, ":set hls");
    expect(view.state.field(vimField).cmdline).toBe(":set hls");
    expect(view.dom.querySelector(".cm-vim-cmdline")?.textContent).toContain(":set hls");
    type(view, "<Esc>");
    expect(view.state.field(vimField).cmdline).toBeNull();
  });

  it("runs IDE actions and writes the clipboard register", async () => {
    const view = await editor("|x");
    type(view, ":action GotoClass<CR>");
    await flush();
    expect(actions).toContain("GotoClass");
    type(view, '"+yy');
    await flush();
    expect(copied).toContain("x\n");
  });

  it("reports the mode and pending keys", async () => {
    const view = await editor("|x");
    type(view, "2d");
    expect(last?.showcmd).toBe("2d");
    type(view, "<Esc>i");
    expect(last?.mode).toBe("insert");
    expect(view.dom.classList.contains("cm-vim-insert")).toBe(true);
  });

  it("detaches when the emulation is turned off", async () => {
    const view = await editor("|x");
    enabled = false;
    refreshVimEditors();
    expect(view.state.field(vimField).active).toBe(false);
    type(view, "dd");
    expect(text(view)).toBe("ddx");
  });

  it("takes keys from the IDE keymap only in a focused Vim editor", async () => {
    await installKeymap(window);
    await setKeymapId("$default");
    await resetKeymap();
    const replace = vi.fn();
    registerCommand({ id: "Replace", title: "Replace", run: replace });
    const view = await editor("|x");
    const press = (init: KeyboardEventInit) => {
      const e = new KeyboardEvent("keydown", { ...init, bubbles: true, cancelable: true });
      view.contentDOM.dispatchEvent(e);
      return e;
    };
    // jsdom does not focus contenteditable elements; tell the engine directly.
    vimControllerOf(view)!.setFocus(true);
    // Ctrl+R is Redo for Vim, not Replace.
    press({ key: "r", code: "KeyR", ctrlKey: true });
    expect(replace).not.toHaveBeenCalled();
    // With the IDE handler the keymap runs Replace.
    vimEngine.configure(true, [{ shortcut: "Ctrl+R", normal: "ide", insert: "ide", visual: "ide" }]);
    press({ key: "r", code: "KeyR", ctrlKey: true });
    await flush();
    expect(replace).toHaveBeenCalledOnce();
  });
});
