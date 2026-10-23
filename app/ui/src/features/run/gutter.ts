/**
 * Green ▶ gutter icons on tests: PHPUnit classes and methods, Pest `test()` / `it()` /
 * `describe()` calls and dataset rows, Codeception Cest/Cept files and methods, Behat
 * features, scenarios and example rows, PHPSpec specs and examples. The tests of a file and
 * their temporary configurations come from Rust (`test_discover`). Clicking an icon offers
 * Run / Debug and Modify Run Configuration; after a run the icon shows the test's last
 * result.
 */
import { gutter, GutterMarker, ViewPlugin, type EditorView, type ViewUpdate } from "@codemirror/view";
import { StateEffect, StateField, type EditorState, type Extension } from "@codemirror/state";
import { Bug, Play, Settings2 } from "lucide-solid";
import { shortcutLabel } from "@/keymap";
import { testFrameworksApi, type TestFile, type TestTarget } from "@/ipc/testFrameworks";
import type { TestStatus } from "./tests";
import { onTestResults } from "./testResults";
import { openMenu } from "./PopupMenu";
import { useTemporary } from "./configStore";
import { debugConfiguration, launch } from "./runStore";
import { openRunConfigurations } from "./dialogState";

/** Last results by result key (`path`, `path::test`). */
const results = new Map<string, TestStatus>();
const views = new Set<EditorView>();
const refresh = StateEffect.define<null>();

onTestResults((entries) => {
  for (const [k, v] of entries) results.set(k, v);
  for (const v of views) v.dispatch({ effects: refresh.of(null) });
});

interface Placed {
  /** Start of the target's line, mapped through edits until the next discovery. */
  pos: number;
  target: TestTarget;
}

interface Discovered {
  placed: Placed[];
  file?: TestFile["file"];
}

const setTests = StateEffect.define<Discovered>();

const testsField = StateField.define<Discovered>({
  create: () => ({ placed: [] }),
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setTests)) return e.value;
    if (!tr.docChanged) return value;
    return { ...value, placed: value.placed.map((p) => ({ ...p, pos: tr.changes.mapPos(p.pos) })) };
  },
});

const SVG = {
  play: '<polygon points="6 3 20 12 6 21 6 3" fill="currentColor"/>',
  passed:
    '<polygon points="4 2 17 10 4 18 4 2" fill="currentColor"/><circle cx="17" cy="17" r="7" fill="var(--bg-editor)"/><circle cx="17" cy="17" r="5.5" fill="currentColor"/><path d="m14.2 17 2 2 3.4-3.6" stroke="var(--bg-editor)" stroke-width="2" fill="none" stroke-linecap="round"/>',
  failed:
    '<polygon points="4 2 17 10 4 18 4 2" fill="var(--success)"/><circle cx="17" cy="17" r="7" fill="var(--bg-editor)"/><circle cx="17" cy="17" r="5.5" fill="currentColor"/><path d="m14.8 14.8 4.4 4.4m0-4.4-4.4 4.4" stroke="var(--bg-editor)" stroke-width="2" fill="none" stroke-linecap="round"/>',
};

const isSingle = (t: TestTarget) => !["class", "file", "feature", "describe"].includes(t.kind);

class RunMarker extends GutterMarker {
  constructor(
    readonly target: TestTarget,
    readonly status: TestStatus | undefined,
  ) {
    super();
  }
  eq(other: RunMarker) {
    return other.target.key === this.target.key && other.target.label === this.target.label && other.status === this.status;
  }
  toDOM() {
    const el = document.createElement("span");
    const state = this.status === "failed" ? "failed" : this.status === "passed" ? "passed" : "play";
    el.className = `run-gutter-icon ${state}`;
    el.title = isSingle(this.target) ? "Run Test" : "Run Tests";
    el.innerHTML = `<svg width="15" height="15" viewBox="0 0 24 24" stroke-linejoin="round">${SVG[state]}</svg>`;
    return el;
  }
}

function showMenu(target: TestTarget, x: number, y: number) {
  const cfg = target.configuration;
  openMenu({
    x,
    y,
    items: [
      {
        label: `Run '${target.label}'`,
        icon: Play,
        iconColor: "var(--success)",
        shortcut: shortcutLabel("RunClass"),
        run: () => void launch(useTemporary(cfg)),
      },
      {
        label: `Debug '${target.label}'`,
        icon: Bug,
        iconColor: "var(--success)",
        shortcut: shortcutLabel("DebugClass"),
        run: () => void debugConfiguration(useTemporary(cfg)),
      },
      { separator: true },
      {
        label: "Modify Run Configuration…",
        icon: Settings2,
        run: () => openRunConfigurations(useTemporary(cfg).name),
      },
    ],
  });
}

/** The tests of the editor (0-based lines as of now) and the configuration for the whole file. */
export function testsOf(state: EditorState): { targets: TestTarget[]; file?: TestFile["file"] } {
  const d = state.field(testsField, false);
  if (!d) return { targets: [] };
  return {
    targets: d.placed.map((p) => ({ ...p.target, line: state.doc.lineAt(Math.min(p.pos, state.doc.length)).number - 1 })),
    file: d.file,
  };
}

function placedAt(view: EditorView, from: number): Placed | undefined {
  const line = view.state.doc.lineAt(from);
  return view.state.field(testsField).placed.find((p) => p.pos >= line.from && p.pos <= line.to);
}

/** Ask Rust for the tests of the document; results for an outdated text are dropped. */
async function discover(view: EditorView, path: string): Promise<void> {
  const doc = view.state.doc;
  const file = await testFrameworksApi.discover(path, doc.toString()).catch(() => null);
  if (!file || view.state.doc !== doc || !views.has(view)) return;
  const placed = file.targets
    .filter((t) => t.line < doc.lines)
    .map((target) => ({ pos: doc.line(target.line + 1).from, target }));
  view.dispatch({ effects: setTests.of({ placed, file: file.file }) });
}

/** CodeMirror extension for a test file at `path` (PHP or `.feature`). */
export function testGutter(path: string): Extension {
  return [
    testsField,
    ViewPlugin.define((view) => {
      views.add(view);
      let timer: ReturnType<typeof setTimeout> | undefined;
      const schedule = (delay: number) => {
        clearTimeout(timer);
        timer = setTimeout(() => void discover(view, path), delay);
      };
      schedule(0);
      return {
        update: (u: ViewUpdate) => {
          if (u.docChanged) schedule(400);
        },
        destroy: () => {
          clearTimeout(timer);
          views.delete(view);
        },
      };
    }),
    gutter({
      class: "cm-run-gutter",
      lineMarker(view, line) {
        const p = placedAt(view, line.from);
        if (!p) return null;
        return new RunMarker(p.target, p.target.key ? results.get(p.target.key) : undefined);
      },
      lineMarkerChange: (update) =>
        update.transactions.some((tr) => tr.effects.some((e) => e.is(refresh) || e.is(setTests))),
      domEventHandlers: {
        mousedown(view, line, event) {
          const p = placedAt(view, line.from);
          if (!p) return false;
          const e = event as MouseEvent;
          showMenu(p.target, e.clientX, e.clientY + 6);
          return true;
        },
      },
    }),
  ];
}
