/**
 * Color previews: swatches in the gutter (a line marker per color, PHPStorm's "Color
 * preview" gutter icons) and, optionally, inline before each color. Clicking a swatch opens
 * the color picker. The colors are found in Rust (`colors_find`, in-process as WebAssembly
 * so swatches follow every edit).
 */
import { RangeSetBuilder, type Extension } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, WidgetType, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { createEffect, createRoot, on } from "solid-js";
import { getSetting } from "@/state/settings";
import { languageIdFor } from "@/editor/languages";
import { registerLineMarkerProvider, type LineMarker } from "@/editor/lineMarkers";
import { colorsApi, type ColorMatch } from "@/ipc/emmet";
import { openColorPicker } from "./ColorPicker";

export const GUTTER_KEY = "editor.colorPreview.gutter";
export const INLINE_KEY = "editor.colorPreview.inline";

const LANGUAGES = new Set([
  "css", "scss", "less", "sass", "stylus", "html", "xhtml", "vue", "blade", "twig", "php", "smarty", "latte",
  "jinja", "handlebars", "gotemplate", "javascript", "typescript", "jsx", "tsx",
]);

/** Whether colors are looked for in files of this type. */
export const hasColors = (path: string): boolean => LANGUAGES.has(languageIdFor(path) ?? "");

/** Colors of a document, in-process when the WebAssembly module is loaded. */
export async function findColors(path: string, text: string): Promise<ColorMatch[]> {
  const request = { text, languageId: languageIdFor(path) };
  return colorsApi.findSync(request) ?? (await colorsApi.find(request));
}

/** Open the picker for a color of the document. */
export function pickColor(view: EditorView, m: ColorMatch, at?: { x: number; y: number }): void {
  const c = at ?? (() => {
    const r = view.coordsAtPos(m.from);
    return { x: r?.left ?? 200, y: (r?.bottom ?? 200) + 4 };
  })();
  openColorPicker({ view, from: m.from, to: m.to, original: m.text, color: m.color, editable: m.editable, x: c.x, y: c.y });
}

/** The swatch drawn for a color (the color itself over a checkerboard for transparency). */
function swatch(hex: string, className: string): HTMLElement {
  const el = document.createElement("span");
  el.className = className;
  const fill = document.createElement("span");
  fill.style.backgroundColor = hex;
  el.append(fill);
  return el;
}

/** Line of each UTF-16 offset (0-based), for offsets in ascending order. */
function linesOf(text: string, offsets: number[]): number[] {
  let line = 0;
  let pos = 0;
  return offsets.map((o) => {
    while (pos < o) {
      const nl = text.indexOf("\n", pos);
      if (nl < 0 || nl >= o) {
        pos = o;
        break;
      }
      line++;
      pos = nl + 1;
    }
    return line;
  });
}

registerLineMarkerProvider({
  appliesTo: hasColors,
  watch: () => getSetting<boolean>(GUTTER_KEY, true),
  markers: async (path, text) => {
    if (!getSetting<boolean>(GUTTER_KEY, true)) return [];
    const found = await findColors(path, text);
    const lines = linesOf(text, found.map((m) => m.from));
    const seen = new Map<number, number>();
    return found.map((m, i): LineMarker => {
      const line = lines[i];
      const nth = seen.get(line) ?? 0;
      seen.set(line, nth + 1);
      const Icon = () => swatch(m.hex, "color-swatch");
      return {
        line,
        icon: Icon,
        title: m.editable ? `${m.text} (click to choose a color)` : m.text,
        className: "marker-color",
        onClick: (view, event) => {
          // Pick the color on that line in the current text (it may have moved since).
          const doc = view.state.doc;
          if (line + 1 > doc.lines) return;
          const l = doc.line(line + 1);
          void findColors(path, doc.toString()).then((now) => {
            const onLine = now.filter((c) => c.from >= l.from && c.from <= l.to);
            const target = onLine[nth] ?? onLine[0];
            if (!target) return;
            const r = (event.target as HTMLElement).getBoundingClientRect();
            pickColor(view, target, { x: r.right + 4, y: r.top });
          });
        },
      };
    });
  },
});

class InlineSwatch extends WidgetType {
  constructor(readonly match: ColorMatch) {
    super();
  }
  eq(other: InlineSwatch): boolean {
    return other.match.hex === this.match.hex && other.match.from === this.match.from;
  }
  toDOM(view: EditorView): HTMLElement {
    const el = swatch(this.match.hex, "cm-color-inline");
    el.title = this.match.text;
    el.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
      const pos = view.posAtDOM(el);
      void findColors(currentPath.get(view) ?? "", view.state.doc.toString()).then((now) => {
        const m = now.find((c) => c.from === pos);
        if (m) pickColor(view, m);
      });
    });
    return el;
  }
  ignoreEvent(): boolean {
    return true;
  }
}

const currentPath = new WeakMap<EditorView, string>();

function inlineDecorations(found: ColorMatch[]): DecorationSet {
  const b = new RangeSetBuilder<Decoration>();
  for (const m of found) b.add(m.from, m.from, Decoration.widget({ widget: new InlineSwatch(m), side: -1 }));
  return b.finish();
}

/** Settings | Editor | General | Appearance | Show color preview inline. */
export function inlineColorPreview(path: string): Extension | null {
  if (!hasColors(path)) return null;
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet = Decoration.none;
      timer: ReturnType<typeof setTimeout> | undefined;
      seq = 0;
      dispose: () => void;
      constructor(readonly view: EditorView) {
        currentPath.set(view, path);
        this.dispose = createRoot((d) => {
          createEffect(on(() => getSetting<boolean>(INLINE_KEY, false), () => this.schedule(0)));
          return d;
        });
      }
      schedule(ms: number) {
        clearTimeout(this.timer);
        this.timer = setTimeout(() => void this.refresh(), ms);
      }
      async refresh() {
        const n = ++this.seq;
        const found = getSetting<boolean>(INLINE_KEY, false) ? await findColors(path, this.view.state.doc.toString()).catch(() => []) : [];
        if (n !== this.seq) return;
        this.decorations = inlineDecorations(found);
        this.view.dispatch({});
      }
      update(u: ViewUpdate) {
        if (!u.docChanged) return;
        this.decorations = this.decorations.map(u.changes);
        this.schedule(200);
      }
      destroy() {
        clearTimeout(this.timer);
        this.seq++;
        this.dispose();
      }
    },
    { decorations: (v) => v.decorations },
  );
}
