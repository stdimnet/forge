/**
 * Partial commits in the editor and the commit diff, like PHPStorm's line status checkboxes:
 * a checkbox at the first line of every change hunk includes it in or excludes it from the
 * next commit. The hunks come from Rust (`git_file_hunks`, `forge_vcs::hunks`) for the
 * document's current text, so unsaved edits count.
 */
import { createEffect, createRoot, on } from "solid-js";
import { RangeSet, StateEffect, StateField, type Extension, type Range } from "@codemirror/state";
import { EditorView, GutterMarker, ViewPlugin, gutter, type ViewUpdate } from "@codemirror/view";
import { changesApi, type ChangeHunk } from "./changesApi";
import { excludedHunks, includedFiles, isHunkIncluded, setHunkIncluded } from "./changelists";
import { revision } from "./store";

const setHunksEffect = StateEffect.define<ChangeHunk[]>();
/** Inclusion changed: redraw the checkboxes. */
const redrawEffect = StateEffect.define<null>();

const hunksField = StateField.define<ChangeHunk[]>({
  create: () => [],
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setHunksEffect)) return e.value;
    return value;
  },
});

/** The hunks the partial-commit gutter shows for a view (tests, the change popup). */
export const viewHunks = (view: EditorView): ChangeHunk[] => view.state.field(hunksField, false) ?? [];

class HunkCheckbox extends GutterMarker {
  constructor(
    readonly id: string,
    readonly included: boolean,
  ) {
    super();
  }
  eq(o: HunkCheckbox) {
    return o.id === this.id && o.included === this.included;
  }
  toDOM() {
    const box = document.createElement("input");
    box.type = "checkbox";
    box.className = "cm-git-hunk-check";
    box.checked = this.included;
    box.dataset.hunk = this.id;
    box.title = this.included ? "Included in commit (click to exclude)" : "Excluded from commit (click to include)";
    box.tabIndex = -1;
    return box;
  }
}

/** The 0-based line a hunk's checkbox sits on. */
export function hunkLine(h: ChangeHunk, lines: number): number {
  return Math.max(0, Math.min(h.newStart, lines - 1));
}

function partialGutter(path: string): Extension {
  return gutter({
    class: "cm-git-partial-gutter",
    markers(view) {
      const doc = view.state.doc;
      const ranges: Range<GutterMarker>[] = [];
      for (const h of view.state.field(hunksField)) {
        const line = doc.line(hunkLine(h, doc.lines) + 1);
        ranges.push(new HunkCheckbox(h.id, isHunkIncluded(path, h.id)).range(line.from));
      }
      return RangeSet.of(ranges, true);
    },
    lineMarkerChange: (u: ViewUpdate) => u.transactions.some((t) => t.effects.some((e) => e.is(setHunksEffect) || e.is(redrawEffect))),
    domEventHandlers: {
      mousedown(view, _block, event) {
        const box = (event.target as HTMLElement).closest<HTMLInputElement>(".cm-git-hunk-check");
        if (!box?.dataset.hunk) return false;
        event.preventDefault();
        const all = view.state.field(hunksField).map((h) => h.id);
        setHunkIncluded(path, box.dataset.hunk, !box.checked, all);
        return true;
      },
    },
  });
}

/** Fetches the hunks for the document (debounced) and redraws when the inclusion changes. */
function hunkLoader(path: string) {
  return ViewPlugin.fromClass(
    class {
      timer: ReturnType<typeof setTimeout> | undefined;
      disposed = false;
      dispose: () => void = () => {};
      constructor(readonly view: EditorView) {
        createRoot((d) => {
          this.dispose = d;
          createEffect(on(revision, () => this.schedule(0)));
          createEffect(
            on(
              () => [includedFiles().has(path), excludedHunks()[path]],
              () => setTimeout(() => this.disposed || this.view.dispatch({ effects: redrawEffect.of(null) })),
              { defer: true },
            ),
          );
        });
      }
      schedule(ms: number) {
        clearTimeout(this.timer);
        this.timer = setTimeout(() => void this.load(), ms);
      }
      async load() {
        const text = this.view.state.doc.toString();
        const hunks = await changesApi.fileHunks(path, text).catch(() => [] as ChangeHunk[]);
        if (!this.disposed && this.view.state.doc.toString() === text) this.view.dispatch({ effects: setHunksEffect.of(hunks) });
      }
      update(u: ViewUpdate) {
        if (u.docChanged) this.schedule(400);
      }
      destroy() {
        this.disposed = true;
        clearTimeout(this.timer);
        this.dispose();
      }
    },
  );
}

const theme = EditorView.baseTheme({
  ".cm-git-partial-gutter": { width: "16px" },
  ".cm-git-partial-gutter .cm-gutterElement": { padding: "0", display: "flex", alignItems: "center", justifyContent: "center" },
  ".cm-git-hunk-check": { margin: "0", width: "12px", height: "12px", cursor: "pointer", accentColor: "var(--accent)" },
});

/** Hunk checkboxes for a view whose document is the working copy of `path`. */
export function partialCommitGutter(path: string): Extension {
  return [hunksField, partialGutter(path), hunkLoader(path), theme];
}
