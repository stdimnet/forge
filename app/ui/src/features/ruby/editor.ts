/**
 * RubyForge in the editor:
 *
 * - ▶ on Ruby scripts (`#!/usr/bin/env ruby`, `if __FILE__ == $0`): Run / Debug / Modify
 *   Run Configuration (`ruby_file_targets`); tests get their ▶ from the shared test runner
 *   (`rspec`, `minitest`);
 * - RuboCop as the external annotator: offenses in the editor and the Problems tool
 *   window after opening, a pause in typing and saving (`ruby.rubocop.onTheFly`; trusted
 *   projects only), Alt+Enter "RuboCop: autocorrect" on correctable offenses;
 * - Reformat Code with RuboCop's autocorrect (`ruby.formatter`), or Ruby LSP.
 */
import { createEffect, createRoot, on } from "solid-js";
import { ViewPlugin, type ViewUpdate } from "@codemirror/view";
import { Bug, Play, Settings2 } from "lucide-solid";
import type { EditorDiagnostic } from "@/ipc/lsp";
import type { ContextAction } from "@/ipc/codeintel";
import { rubyApi, type RubyProblem } from "@/ipc/ruby";
import { registerEditorExtension } from "@/editor/extensions";
import { registerLineMarkerProvider } from "@/editor/lineMarkers";
import { viewFor } from "@/editor/views";
import { registerContextActionHandler, registerContextActionProvider } from "@/features/codeintel/intentions";
import { publishDiagnostics } from "@/features/lsp/store";
import { registerFormatProvider, replaceChangedPart } from "@/features/lsp/format";
import { openMenu } from "@/features/run/PopupMenu";
import { useTemporary } from "@/features/run/configStore";
import { debugConfiguration, launch } from "@/features/run/runStore";
import { openRunConfigurations } from "@/features/run/dialogState";
import { safeMode } from "@/features/trust/store";
import { shortcutLabel } from "@/keymap";
import { editor, onFileSaved } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { errorText, rubyActive, showRubyBalloon } from "./state";

const fileName = (path: string) => path.replace(/\\/g, "/").split("/").pop() ?? path;
const RUBY_NAMES = new Set(["Gemfile", "gems.rb", "Rakefile", "rakefile", "Guardfile", "Capfile", "Vagrantfile", "Brewfile", ".irbrc", ".pryrc"]);
export const isRubyPath = (path: string) => /\.(rb|rake|gemspec|ru|rbw|jbuilder)$/.test(path) || RUBY_NAMES.has(fileName(path));

// ---- ▶ on scripts -------------------------------------------------------------------------

registerLineMarkerProvider({
  appliesTo: (path) => isRubyPath(path) || /(^|\/)bin\/[^/.]+$/.test(path),
  watch: () => rubyActive(),
  markers: async (path, text) => {
    if (!rubyActive() || !(/^#!.*ruby/.test(text) || /__FILE__|\$PROGRAM_NAME/.test(text))) return [];
    const targets = await rubyApi.fileTargets(path, text).catch(() => []);
    return targets.map((t) => {
      const label = t.configuration.name;
      return {
        line: t.line,
        icon: Play,
        title: `Run '${label}'`,
        className: "marker-run",
        onClick: (_view, event) => {
          const cfg = t.configuration;
          openMenu({
            x: event.clientX,
            y: event.clientY,
            items: [
              { label: `Run '${label}'`, icon: Play, iconColor: "var(--success)", shortcut: shortcutLabel("RunClass"), run: () => void launch(useTemporary(cfg)) },
              { label: `Debug '${label}'`, icon: Bug, iconColor: "var(--success)", shortcut: shortcutLabel("DebugClass"), run: () => void debugConfiguration(useTemporary(cfg)) },
              { separator: true },
              { label: "Modify Run Configuration…", icon: Settings2, run: () => openRunConfigurations(useTemporary(cfg).name) },
            ],
          });
        },
      };
    });
  },
});

// ---- RuboCop --------------------------------------------------------------------------------

const SOURCE = "rubocop";
const problemsByFile = new Map<string, RubyProblem[]>();
const onTheFly = () => getSetting<boolean>("ruby.rubocop.onTheFly", true) !== false;
let warnedMissing = false;

export function publishRubocop(path: string, problems: RubyProblem[]): void {
  problemsByFile.set(path, problems);
  publishDiagnostics(
    path,
    problems.map(
      (p): EditorDiagnostic => ({
        range: { start: { line: p.line, character: p.column }, end: { line: p.endLine, character: p.endColumn } },
        severity: p.severity,
        message: p.message,
        source: "RuboCop",
        code: p.cop,
      }),
    ),
    SOURCE,
  );
}

/** Check a Ruby file with RuboCop (the editor text when given). */
export async function checkWithRubocop(path: string, text?: string, quiet = true): Promise<RubyProblem[] | null> {
  if (!rubyActive() || safeMode() || !/\.(rb|rake|gemspec|ru)$|(^|\/)(Gemfile|Rakefile)$/.test(path)) return null;
  try {
    const problems = await rubyApi.rubocop(path, text);
    publishRubocop(path, problems);
    if (!quiet) showRubyBalloon("RuboCop", problems.length ? `${problems.length} offense${problems.length === 1 ? "" : "s"} in ${fileName(path)}` : "No offenses", 4000);
    return problems;
  } catch (e) {
    const msg = errorText(e);
    if (!quiet || (!warnedMissing && /not found|No such file|cannot load|not installed/i.test(msg))) {
      warnedMissing = true;
      showRubyBalloon("RuboCop", msg, 8000);
    }
    return null;
  }
}

/** Pause after the last keystroke before RuboCop checks the text. */
const DELAY_MS = 1200;

registerEditorExtension(({ path }) => {
  if (!isRubyPath(path)) return null;
  return ViewPlugin.define((view) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const opened = setTimeout(() => onTheFly() && void checkWithRubocop(path, view.state.doc.toString()), 400);
    return {
      update(u: ViewUpdate) {
        if (!u.docChanged || !onTheFly()) return;
        clearTimeout(timer);
        timer = setTimeout(() => void checkWithRubocop(path, view.state.doc.toString()), DELAY_MS);
      },
      destroy() {
        clearTimeout(timer);
        clearTimeout(opened);
      },
    };
  });
});

/** Autocorrect a file with RuboCop (`all`: also unsafe corrections) in the editor. */
export async function autocorrect(path: string, mode: "safe" | "all" = "safe"): Promise<void> {
  const view = viewFor(path);
  if (!view) return;
  const text = view.state.doc.toString();
  let fixed: string | null;
  try {
    fixed = await rubyApi.format(path, text, mode);
  } catch (e) {
    showRubyBalloon("RuboCop autocorrect failed", errorText(e), 8000);
    return;
  }
  if (fixed == null || fixed === text) {
    showRubyBalloon("RuboCop: nothing to correct", fileName(path), 3000);
    return;
  }
  if (view.state.doc.toString() !== text) return;
  replaceChangedPart(view, fixed, "input.fix");
  void checkWithRubocop(path, view.state.doc.toString());
}

registerContextActionProvider(async (path, view) => {
  if (!rubyActive() || !isRubyPath(path)) return [];
  const line = view.state.doc.lineAt(view.state.selection.main.head).number - 1;
  const here = (problemsByFile.get(path) ?? []).filter((p) => p.correctable && line >= p.line && line <= p.endLine);
  if (!here.length) return [];
  const cops = [...new Set(here.map((p) => p.cop))].join(", ");
  return [
    {
      title: `RuboCop: autocorrect ${cops}`,
      kind: "quickFix",
      source: SOURCE,
      isPreferred: true,
      needsResolve: false,
      action: "Ruby.RubocopAutocorrect",
      data: { mode: "safe" },
    },
    {
      title: "RuboCop: autocorrect all offenses in the file (also unsafe)",
      kind: "quickFix",
      source: SOURCE,
      isPreferred: false,
      needsResolve: false,
      action: "Ruby.RubocopAutocorrect",
      data: { mode: "all" },
    },
  ] satisfies ContextAction[];
});

registerContextActionHandler("Ruby.RubocopAutocorrect", (data, ctx) => autocorrect(ctx.path, (data as { mode: "safe" | "all" }).mode));

// ---- Reformat Code ----------------------------------------------------------------------------

registerFormatProvider(async (path, text) => {
  if (!isRubyPath(path) || !rubyActive()) return null;
  const formatter = getSetting<string>("ruby.formatter", "rubocop");
  if (formatter === "none") return text;
  if (formatter === "lsp") return null;
  try {
    return (await rubyApi.format(path, text)) ?? null;
  } catch (e) {
    showRubyBalloon("RuboCop", errorText(e), 6000);
    return text;
  }
});

if (typeof document !== "undefined") {
  onFileSaved((path) => {
    if (!isRubyPath(path) || !rubyActive() || !onTheFly()) return;
    setTimeout(() => void checkWithRubocop(path, viewFor(path)?.state.doc.toString()), 200);
  });
  // Safe Mode ended or RuboCop's settings changed: check the open Ruby files again.
  createRoot(() =>
    createEffect(
      on(
        () => [safeMode(), onTheFly(), getSetting("ruby.rubocop.options", ""), getSetting("ruby.rubocop.path", "")],
        () => {
          for (const tab of editor.tabs) {
            const view = viewFor(tab.path);
            if (isRubyPath(tab.path) && view) {
              if (!onTheFly() || safeMode()) publishRubocop(tab.path, []);
              else void checkWithRubocop(tab.path, view.state.doc.toString());
            }
          }
        },
        { defer: true },
      ),
    ),
  );
}
