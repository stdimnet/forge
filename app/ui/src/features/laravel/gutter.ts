/**
 * Gutter icons like Laravel Idea's: routed controller methods (globe, goes to the route),
 * route definitions (goes to the controller method) and `view()` calls / Blade includes
 * (goes to the template). Refreshed while typing, debounced.
 */
import { RangeSet, StateEffect, StateField, type Extension } from "@codemirror/state";
import { EditorView, gutter, GutterMarker, ViewPlugin, type ViewUpdate } from "@codemirror/view";
import { Globe, LayoutTemplate, SquareArrowOutUpRight } from "lucide-solid";
import { laravelApi, type GutterMark } from "@/ipc/laravel";
import { iconNode } from "@/features/lsp/util";
import { laravelActive, laravelSetting, reveal } from "./state";
import { laravelGoto } from "./goto";

const ICONS = { route: Globe, controller: SquareArrowOutUpRight, view: LayoutTemplate } as const;

class LaravelMarker extends GutterMarker {
  constructor(readonly mark: GutterMark) {
    super();
  }
  eq(other: LaravelMarker) {
    return other.mark.kind === this.mark.kind && other.mark.tooltip === this.mark.tooltip && other.mark.line === this.mark.line;
  }
  toDOM() {
    const el = document.createElement("span");
    el.className = `laravel-gutter-icon kind-${this.mark.kind}`;
    el.title = this.mark.tooltip;
    el.dataset.laravelGutter = this.mark.kind;
    el.append(iconNode(ICONS[this.mark.kind], 13));
    return el;
  }
}

const setMarks = StateEffect.define<GutterMark[]>();

const marksField = StateField.define<RangeSet<GutterMarker>>({
  create: () => RangeSet.empty,
  update(set, tr) {
    for (const e of tr.effects) {
      if (e.is(setMarks)) {
        const doc = tr.state.doc;
        const ranges = e.value
          .filter((m) => m.line < doc.lines)
          .map((m) => new LaravelMarker(m).range(doc.line(m.line + 1).from));
        return RangeSet.of(ranges, true);
      }
    }
    return tr.docChanged ? set.map(tr.changes) : set;
  },
});

function enabledKinds(): Set<string> {
  const kinds = new Set<string>();
  if (laravelSetting("laravel.gutter.routes", true)) kinds.add("route").add("controller");
  if (laravelSetting("laravel.gutter.views", true)) kinds.add("view");
  return kinds;
}

function refresher(path: string) {
  return ViewPlugin.fromClass(
    class {
      timer: ReturnType<typeof setTimeout> | undefined;
      constructor(readonly view: EditorView) {
        this.schedule(50);
      }
      update(u: ViewUpdate) {
        if (u.docChanged) this.schedule(700);
      }
      schedule(ms: number) {
        clearTimeout(this.timer);
        this.timer = setTimeout(() => void this.refresh(), ms);
      }
      async refresh() {
        if (!laravelActive()) return;
        const kinds = enabledKinds();
        if (!kinds.size) return;
        const marks = await laravelApi.gutter(path, this.view.state.doc.toString()).catch(() => []);
        if (!this.view.dom.isConnected && !this.view.state) return;
        this.view.dispatch({ effects: setMarks.of(marks.filter((m) => kinds.has(m.kind))) });
      }
      destroy() {
        clearTimeout(this.timer);
      }
    },
  );
}

export function laravelGutter(path: string): Extension {
  return [
    marksField,
    refresher(path),
    gutter({
      class: "cm-laravel-gutter",
      markers: (view) => view.state.field(marksField),
      domEventHandlers: {
        mousedown(view, block) {
          let mark: GutterMark | undefined;
          view.state.field(marksField).between(block.from, block.from, (_f, _t, m) => {
            mark = (m as LaravelMarker).mark;
          });
          if (!mark) return false;
          if (mark.targets.length === 1) void reveal(mark.targets[0]);
          else if (mark.targets.length > 1) {
            // Reuse the "Choose Declaration" list of Go to Declaration.
            const pos = block.from + view.state.doc.lineAt(block.from).text.search(/\S|$/);
            void laravelGoto(path, view, pos).then((ok) => ok || reveal(mark!.targets[0]));
          }
          return true;
        },
      },
    }),
  ];
}
