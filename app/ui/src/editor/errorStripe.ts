/**
 * PHPStorm's error stripe: small marks on the right edge of the editor (over the
 * scrollbar) for places in the whole file, e.g. highlighted usages. Features contribute
 * marks through the `stripeMarks` facet; clicking a mark moves the caret there.
 */
import { EditorSelection, Facet } from "@codemirror/state";
import { EditorView, ViewPlugin, type ViewUpdate } from "@codemirror/view";

export interface StripeMark {
  /** Document offset. */
  pos: number;
  /** CSS class with the color (`cm-stripe-usage-read`). */
  cls: string;
  title?: string;
}

/** Marks of the error stripe; every provider adds a list. */
export const stripeMarks = Facet.define<readonly StripeMark[], readonly StripeMark[]>({
  combine: (lists) => lists.flat(),
});

const MARK_HEIGHT = 3;

class StripeView {
  dom: HTMLDivElement;
  marks: readonly StripeMark[] = [];

  constructor(readonly view: EditorView) {
    this.dom = document.createElement("div");
    this.dom.className = "cm-error-stripe";
    this.dom.setAttribute("aria-hidden", "true");
    view.dom.appendChild(this.dom);
    this.dom.addEventListener("mousedown", this.click);
    this.render();
  }

  update(u: ViewUpdate) {
    const marks = u.state.facet(stripeMarks);
    if (marks !== this.marks || u.geometryChanged || u.heightChanged) this.render();
  }

  click = (e: MouseEvent) => {
    const target = (e.target as HTMLElement).closest<HTMLElement>("[data-pos]");
    if (!target) return;
    e.preventDefault();
    const pos = Math.min(Number(target.dataset.pos), this.view.state.doc.length);
    this.view.dispatch({ selection: EditorSelection.cursor(pos), scrollIntoView: true });
    this.view.focus();
  };

  render() {
    this.marks = this.view.state.facet(stripeMarks);
    this.dom.replaceChildren();
    if (!this.marks.length) {
      this.dom.style.display = "none";
      return;
    }
    this.dom.style.display = "";
    const track = this.view.scrollDOM.clientHeight;
    const total = Math.max(this.view.contentHeight, track);
    const seen = new Set<string>();
    for (const m of this.marks) {
      const top = Math.round((this.view.lineBlockAt(Math.min(m.pos, this.view.state.doc.length)).top / total) * (track - MARK_HEIGHT));
      const key = `${top}:${m.cls}`;
      if (seen.has(key)) continue;
      seen.add(key);
      const el = document.createElement("div");
      el.className = `cm-stripe-mark ${m.cls}`;
      el.style.top = `${top}px`;
      el.dataset.pos = String(m.pos);
      if (m.title) el.title = m.title;
      this.dom.appendChild(el);
    }
  }

  destroy() {
    this.dom.removeEventListener("mousedown", this.click);
    this.dom.remove();
  }
}

const stripeTheme = EditorView.baseTheme({
  ".cm-error-stripe": {
    position: "absolute",
    top: "0",
    right: "0",
    bottom: "0",
    width: "10px",
    zIndex: "5",
    pointerEvents: "none",
  },
  ".cm-stripe-mark": {
    position: "absolute",
    right: "1px",
    width: "8px",
    height: `${MARK_HEIGHT}px`,
    borderRadius: "1px",
    pointerEvents: "auto",
    cursor: "pointer",
  },
  ".cm-stripe-usage-read": { backgroundColor: "var(--usage-read-mark)" },
  ".cm-stripe-usage-write": { backgroundColor: "var(--usage-write-mark)" },
});

/** The error stripe of an editor (once per editor; marks come from `stripeMarks`). */
export const errorStripe = () => [ViewPlugin.fromClass(StripeView), stripeTheme];
