/**
 * Line marker contribution point: gutter icons like PHPStorm's (route on a controller
 * action, "go to template", "go to repository"). Providers compute markers per file from
 * its content; the gutter refreshes after edits.
 *
 *   registerLineMarkerProvider({ appliesTo: (p) => p.endsWith(".php"), markers: async (path, text) => [...] });
 */
import { createEffect, createRoot, on, type Component } from "solid-js";
import { render } from "solid-js/web";
import { RangeSet, StateEffect, StateField, type Extension } from "@codemirror/state";
import { EditorView, GutterMarker, ViewPlugin, gutter } from "@codemirror/view";
import { registerEditorExtension } from "./extensions";

export interface LineMarker {
  /** 0-based line. */
  line: number;
  icon: Component<{ size?: number; "stroke-width"?: number }>;
  title: string;
  /** Extra class for the icon color, e.g. `marker-route`. */
  className?: string;
  onClick?: (view: EditorView, event: MouseEvent) => void;
}

export interface LineMarkerProvider {
  appliesTo: (path: string) => boolean;
  /** Reactive value (e.g. "plugin active"); markers are recomputed when it changes. */
  watch?: () => unknown;
  markers: (path: string, text: string) => Promise<LineMarker[]>;
}

const providers: LineMarkerProvider[] = [];

export function registerLineMarkerProvider(p: LineMarkerProvider): void {
  providers.push(p);
}

class IconMarker extends GutterMarker {
  constructor(readonly markers: LineMarker[]) {
    super();
  }

  eq(other: GutterMarker): boolean {
    return (
      other instanceof IconMarker &&
      other.markers.length === this.markers.length &&
      other.markers.every((m, i) => m.title === this.markers[i].title && m.icon === this.markers[i].icon)
    );
  }

  toDOM(view: EditorView): Node {
    const wrap = document.createElement("span");
    wrap.className = "cm-line-markers";
    for (const m of this.markers.slice(0, 2)) {
      const el = document.createElement("span");
      el.className = `cm-line-marker ${m.className ?? ""}`;
      el.title = m.title;
      createRoot((dispose) => {
        render(() => m.icon({ size: 13, "stroke-width": 2 }), el);
        return dispose;
      });
      el.addEventListener("mousedown", (e) => {
        e.preventDefault();
        e.stopPropagation();
        m.onClick?.(view, e);
      });
      wrap.append(el);
    }
    return wrap;
  }
}

const setMarkers = StateEffect.define<LineMarker[]>();

const markerField = StateField.define<LineMarker[]>({
  create: () => [],
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setMarkers)) return e.value;
    if (!tr.docChanged) return value;
    // Keep markers on their lines while the new ones are computed.
    return value
      .map((m) => {
        if (m.line + 1 > tr.startState.doc.lines) return null;
        const pos = tr.changes.mapPos(tr.startState.doc.line(m.line + 1).from, 1);
        return { ...m, line: tr.state.doc.lineAt(pos).number - 1 };
      })
      .filter((m): m is LineMarker => m !== null);
  },
});

function markerSet(view: EditorView) {
  const byLine = new Map<number, LineMarker[]>();
  for (const m of view.state.field(markerField)) {
    if (m.line + 1 > view.state.doc.lines) continue;
    byLine.set(m.line, [...(byLine.get(m.line) ?? []), m]);
  }
  const ranges = [...byLine.entries()]
    .sort((a, b) => a[0] - b[0])
    .map(([line, ms]) => new IconMarker(ms).range(view.state.doc.line(line + 1).from));
  return RangeSet.of(ranges);
}

function lineMarkers(path: string, active: LineMarkerProvider[]): Extension {
  const refresher = ViewPlugin.define((view) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let seq = 0;
    const refresh = () => {
      const n = ++seq;
      const text = view.state.doc.toString();
      void Promise.all(active.map((p) => p.markers(path, text).catch(() => [] as LineMarker[]))).then((lists) => {
        if (n !== seq) return;
        view.dispatch({ effects: setMarkers.of(lists.flat()) });
      });
    };
    setTimeout(refresh, 0);
    const dispose = createRoot((d) => {
      createEffect(on(() => active.map((p) => p.watch?.()), () => setTimeout(refresh, 0), { defer: true }));
      return d;
    });
    return {
      update(u) {
        if (!u.docChanged) return;
        clearTimeout(timer);
        timer = setTimeout(refresh, 600);
      },
      destroy() {
        clearTimeout(timer);
        seq++;
        dispose();
      },
    };
  });
  return [
    markerField,
    refresher,
    gutter({
      class: "cm-line-marker-gutter",
      markers: markerSet,
      initialSpacer: () => new IconMarker([]),
      lineMarkerChange: (u) => u.transactions.some((t) => t.effects.some((e) => e.is(setMarkers))) || u.docChanged,
    }),
  ];
}

registerEditorExtension(({ path }) => {
  const active = providers.filter((p) => p.appliesTo(path));
  return active.length ? lineMarkers(path, active) : null;
});
