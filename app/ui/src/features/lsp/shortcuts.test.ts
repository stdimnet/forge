/**
 * Reformat Code, Next/Previous Highlighted Error and Parameter Info run only through the
 * keymap, so a rebound shortcut is the only one that works (#217). The language-server editor
 * extensions bind no action shortcut of their own; the signature popup keeps its Escape.
 */
import { describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { keymap } from "@codemirror/view";
import { lspDiagnostics } from "./diagnostics";
import { lspSignatureHelp } from "./signature";

const boundKeys = (state: EditorState) =>
  state
    .facet(keymap)
    .flat()
    .flatMap((b) => [b.key, b.mac, b.win, b.linux].filter((k): k is string => !!k));

describe("language server editor extensions", () => {
  it("bind no keymap action shortcuts (F2, Shift+F2, Ctrl+P, Ctrl+Alt+L)", () => {
    const state = EditorState.create({ doc: "<?php\n", extensions: [lspDiagnostics("a.php"), lspSignatureHelp("a.php")] });
    const keys = boundKeys(state);
    for (const k of ["F2", "Shift-F2", "Mod-p", "Mod-Alt-l"]) expect(keys).not.toContain(k);
    expect(keys).toEqual(["Escape"]);
  });
});
