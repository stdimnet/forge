/**
 * Color previews and the color picker (PHPStorm's gutter color swatches and "Show Color
 * Picker"): swatches next to colors in CSS, markup and scripts, an optional inline swatch,
 * and the picker that writes the chosen color back in its notation. Finding, parsing and
 * formatting colors is Rust (`crates/forge-colors`).
 */
import { render } from "solid-js/web";
import { registerCommand } from "@/state/commands";
import { registerEditorExtension } from "@/editor/extensions";
import { activeView } from "@/editor/views";
import { editor } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { ColorPickerHost, openColorPicker, RECENT_KEY } from "./ColorPicker";
import { findColors, hasColors, inlineColorPreview, pickColor } from "./preview";
import "./colors.css";

registerEditorExtension(({ path }) => inlineColorPreview(path));

/** The color at the caret in the picker, or a new color inserted at the caret. */
async function showColorPicker(): Promise<void> {
  const view = activeView();
  const path = editor.active;
  if (!view || !path) return;
  const head = view.state.selection.main.head;
  const found = hasColors(path) ? await findColors(path, view.state.doc.toString()) : [];
  const at = found.find((m) => m.from <= head && head <= m.to);
  if (at) {
    pickColor(view, at);
    return;
  }
  const r = view.coordsAtPos(head);
  const last = (getSetting<string[]>(RECENT_KEY, []) ?? [])[0];
  const hex = last ?? "#ffffff";
  const n = parseInt(hex.slice(1, 7), 16) || 0xffffff;
  openColorPicker({
    view,
    from: head,
    to: head,
    original: "",
    color: { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255, a: 1 },
    editable: true,
    x: r?.left ?? 200,
    y: (r?.bottom ?? 200) + 4,
  });
}

registerCommand({ id: "ShowColorPicker", title: "Show Color Picker", category: "Code", run: showColorPicker });

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "color-picker-overlay";
  document.body.appendChild(host);
  render(ColorPickerHost, host);
}
