/**
 * Coverage marks in the editor gutter, like PHPStorm: a green bar on covered lines, yellow on
 * partly covered ones (some branches not taken) and red on lines that never ran; hovering a
 * mark shows the hits ("Hits: 3, branches: 1 of 2 covered"). When the report names the
 * tests of each line (PHPUnit / Codeception per-test coverage, LCOV test names), clicking a
 * mark shows the tests covering the line; clicking a test searches for it. The marks come
 * from Rust (`coverage_file`).
 */
import { gutter, GutterMarker, ViewPlugin, type EditorView } from "@codemirror/view";
import { StateEffect, StateField, type Extension } from "@codemirror/state";
import type { LineMark } from "@/ipc/testing";
import { openSearchEverywhere } from "@/features/search/state";
import { marksFor, onCoverageChange } from "./store";

const setMarks = StateEffect.define<LineMark[]>();

const marksField = StateField.define<Map<number, LineMark>>({
  create: () => new Map(),
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setMarks)) return new Map(e.value.map((m) => [m.line, m]));
    return value;
  },
});

class CoverageMarker extends GutterMarker {
  constructor(readonly mark: LineMark) {
    super();
  }
  eq(other: CoverageMarker) {
    return other.mark.status === this.mark.status && other.mark.hint === this.mark.hint;
  }
  toDOM() {
    const el = document.createElement("span");
    el.className = `coverage-mark ${this.mark.status}${this.mark.tests?.length ? " with-tests" : ""}`;
    el.title = this.mark.tests?.length ? `${this.mark.hint}. Click to show the tests covering this line` : this.mark.hint;
    el.dataset.coverage = this.mark.status;
    return el;
  }
}

const views = new Map<EditorView, string>();

let popup: HTMLElement | null = null;

function closePopup() {
  popup?.remove();
  popup = null;
  document.removeEventListener("mousedown", outside, true);
}

function outside(e: MouseEvent) {
  if (popup && !popup.contains(e.target as Node)) closePopup();
}

/** Search for a test by its reported name: `Tests\CartTest::test_total with data set #0` → the method. */
export function testQuery(name: string): { tab: "symbols" | "all"; query: string } {
  const method = name.includes("::") ? name.split("::").pop()!.replace(/ with data set .*$/, "") : "";
  return method ? { tab: "symbols", query: method } : { tab: "all", query: name };
}

/** The tests covering a line, under the mark. */
export function showLineTests(mark: LineMark, x: number, y: number): void {
  closePopup();
  const el = document.createElement("div");
  el.className = "coverage-line-popup";
  el.setAttribute("role", "dialog");
  el.setAttribute("aria-label", `Tests covering line ${mark.line}`);
  const head = document.createElement("header");
  const swatch = document.createElement("span");
  swatch.className = `swatch coverage-mark ${mark.status}`;
  head.append(swatch, document.createTextNode(`Line ${mark.line}: ${mark.hint}`));
  const caption = document.createElement("div");
  caption.className = "caption";
  caption.textContent = `${mark.tests!.length} test${mark.tests!.length === 1 ? "" : "s"} covering this line`;
  const list = document.createElement("ul");
  for (const t of mark.tests!) {
    const li = document.createElement("li");
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = t;
    b.title = `Search for ${t}`;
    b.onclick = () => {
      closePopup();
      const q = testQuery(t);
      openSearchEverywhere(q.tab, q.query);
    };
    li.append(b);
    list.append(li);
  }
  el.append(head, caption, list);
  el.style.left = `${Math.max(4, Math.min(x, window.innerWidth - 540))}px`;
  el.style.top = `${Math.max(4, Math.min(y, window.innerHeight - 300))}px`;
  document.body.append(el);
  popup = el;
  document.addEventListener("mousedown", outside, true);
}

async function load(view: EditorView, path: string) {
  const list = await marksFor(path);
  if (!views.has(view)) return;
  view.dispatch({ effects: setMarks.of(list) });
}

onCoverageChange(() => {
  for (const [v, path] of views) void load(v, path);
});

/** The coverage gutter of a project file. */
export function coverageGutter(path: string): Extension {
  return [
    marksField,
    ViewPlugin.define((view) => {
      views.set(view, path);
      void load(view, path);
      return { destroy: () => views.delete(view) };
    }),
    gutter({
      class: "cm-coverage-gutter",
      lineMarker(view, line) {
        const n = view.state.doc.lineAt(line.from).number;
        const m = view.state.field(marksField).get(n);
        return m ? new CoverageMarker(m) : null;
      },
      lineMarkerChange: (u) => u.transactions.some((tr) => tr.effects.some((e) => e.is(setMarks))),
      domEventHandlers: {
        mousedown(view, line, event) {
          const m = view.state.field(marksField).get(view.state.doc.lineAt(line.from).number);
          if (!m?.tests?.length) return false;
          const e = event as MouseEvent;
          showLineTests(m, e.clientX + 8, e.clientY + 4);
          e.preventDefault();
          return true;
        },
      },
    }),
  ];
}
