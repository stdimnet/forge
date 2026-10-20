/**
 * Green ▶ gutter icons on PHPUnit test classes and methods. Clicking one offers Run / Debug
 * (temporary configurations) and Modify Run Configuration; after a run the icon shows the
 * test's last result.
 */
import { gutter, GutterMarker, ViewPlugin, type EditorView } from "@codemirror/view";
import { StateEffect, type Extension } from "@codemirror/state";
import { Bug, Play, Settings2 } from "lucide-solid";
import { shortcutLabel } from "@/keymap";
import type { TestStatus } from "./tests";
import { onTestResults } from "./testResults";
import { findTests, testConfiguration, type TestTarget } from "./phpTests";
import { openMenu } from "./PopupMenu";
import { useTemporary } from "./configStore";
import { debugConfiguration, launch } from "./runStore";
import { openRunConfigurations } from "./dialogState";

/** Last results by `Fqcn` or `Fqcn::method`. */
const results = new Map<string, TestStatus>();
const views = new Set<EditorView>();
const refresh = StateEffect.define<null>();

onTestResults((entries) => {
  for (const [k, v] of entries) results.set(k.replace(/^\\/, ""), v);
  for (const v of views) v.dispatch({ effects: refresh.of(null) });
});

const SVG = {
  play: '<polygon points="6 3 20 12 6 21 6 3" fill="currentColor"/>',
  passed:
    '<polygon points="4 2 17 10 4 18 4 2" fill="currentColor"/><circle cx="17" cy="17" r="7" fill="var(--bg-editor)"/><circle cx="17" cy="17" r="5.5" fill="currentColor"/><path d="m14.2 17 2 2 3.4-3.6" stroke="var(--bg-editor)" stroke-width="2" fill="none" stroke-linecap="round"/>',
  failed:
    '<polygon points="4 2 17 10 4 18 4 2" fill="var(--success)"/><circle cx="17" cy="17" r="7" fill="var(--bg-editor)"/><circle cx="17" cy="17" r="5.5" fill="currentColor"/><path d="m14.8 14.8 4.4 4.4m0-4.4-4.4 4.4" stroke="var(--bg-editor)" stroke-width="2" fill="none" stroke-linecap="round"/>',
};

class RunMarker extends GutterMarker {
  constructor(
    readonly target: TestTarget,
    readonly status: TestStatus | undefined,
  ) {
    super();
  }
  eq(other: RunMarker) {
    return other.target.line === this.target.line && other.target.method === this.target.method && other.status === this.status;
  }
  toDOM() {
    const el = document.createElement("span");
    const state = this.status === "failed" ? "failed" : this.status === "passed" ? "passed" : "play";
    el.className = `run-gutter-icon ${state}`;
    el.title = this.target.kind === "method" ? `Run Test` : `Run Tests`;
    el.innerHTML = `<svg width="15" height="15" viewBox="0 0 24 24" stroke-linejoin="round">${SVG[state]}</svg>`;
    return el;
  }
}

function showMenu(target: TestTarget, path: string, x: number, y: number) {
  const cfg = testConfiguration(target, path);
  const label = target.kind === "method" ? target.method! : target.className;
  openMenu({
    x,
    y,
    items: [
      {
        label: `Run '${label}'`,
        icon: Play,
        iconColor: "var(--success)",
        shortcut: shortcutLabel("RunClass"),
        run: () => void launch(useTemporary(cfg)),
      },
      {
        label: `Debug '${label}'`,
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

/** CodeMirror extension for a PHP file at `path`. */
export function testGutter(path: string): Extension {
  let cache: { doc: unknown; tests: TestTarget[] } | null = null;
  const testsOf = (view: EditorView) => {
    if (cache?.doc !== view.state.doc) cache = { doc: view.state.doc, tests: findTests(view.state.doc.toString()) };
    return cache.tests;
  };
  return [
    ViewPlugin.define((view) => {
      views.add(view);
      return { destroy: () => views.delete(view) };
    }),
    gutter({
      class: "cm-run-gutter",
      lineMarker(view, line) {
        const n = view.state.doc.lineAt(line.from).number - 1;
        const t = testsOf(view).find((x) => x.line === n);
        if (!t) return null;
        const key = t.kind === "method" ? `${t.fqcn}::${t.method}` : t.fqcn;
        return new RunMarker(t, results.get(key));
      },
      lineMarkerChange: (update) => update.transactions.some((tr) => tr.effects.some((e) => e.is(refresh))),
      domEventHandlers: {
        mousedown(view, line, event) {
          const n = view.state.doc.lineAt(line.from).number - 1;
          const t = testsOf(view).find((x) => x.line === n);
          if (!t) return false;
          const e = event as MouseEvent;
          showMenu(t, path, e.clientX, e.clientY + 6);
          return true;
        },
      },
    }),
  ];
}
