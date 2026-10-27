/**
 * Alt+Enter on a problem: PHPStorm's inspection options under each problem — "Edit
 * inspection profile setting", "Disable inspection" and "Suppress for statement / function /
 * class / file". Where the comment goes is computed in Rust (`forge_inspections::suppress`).
 */
import type { EditorView } from "@codemirror/view";
import type { ContextAction } from "@/ipc/codeintel";
import { inspectionsApi } from "@/ipc/inspections";
import { diagnostics } from "@/features/lsp/store";
import { toLsp } from "@/features/lsp/util";
import { registerContextActionHandler, registerContextActionProvider } from "@/features/codeintel/intentions";
import { openSettings } from "@/features/settings/dialogState";
import { showBalloon } from "@/keymap";
import { INSPECTIONS_PAGE } from "./draft";
import { profileOp, profiles, selectedProfile, setProfiles, suppressActions } from "./model";

const before = (a: { line: number; character: number }, b: { line: number; character: number }) =>
  a.line < b.line || (a.line === b.line && a.character <= b.character);

function entry(title: string, action: string, data: unknown): ContextAction {
  return { title, kind: "intention", source: "inspections", isPreferred: false, needsResolve: false, action, data };
}

registerContextActionProvider(async (path: string, view: EditorView) => {
  const caret = toLsp(view.state.doc, view.state.selection.main.head);
  const here = (diagnostics[path] ?? []).filter((d) => d.inspection && before(d.range.start, caret) && before(caret, d.range.end));
  const seen = new Set<string>();
  const out: ContextAction[] = [];
  const text = view.state.doc.toString();
  for (const d of here) {
    const id = d.inspection!;
    if (seen.has(id)) continue;
    seen.add(id);
    const name = d.inspectionName ?? id;
    const suppress = suppressActions(path, text, d.range.start.line, d.range.start.character, id).map((a) =>
      entry(a.title, "inspections.suppress", { changes: a.changes }),
    );
    out.push({
      ...entry(`Inspection '${name}' options`, "inspections.edit", { id }),
      children: [entry("Edit inspection profile setting", "inspections.edit", { id }), entry("Disable inspection", "inspections.disable", { id }), ...suppress],
    });
  }
  return out;
});

registerContextActionHandler("inspections.suppress", (data, ctx) => {
  const { changes } = data as { changes: { start: number; end: number; text: string }[] };
  ctx.view.dispatch({
    changes: changes.map((c) => ({ from: c.start, to: c.end, insert: c.text })),
    userEvent: "input.suppress",
  });
});

registerContextActionHandler("inspections.edit", (data) => {
  openSettings(INSPECTIONS_PAGE, { inspection: (data as { id: string }).id });
});

registerContextActionHandler("inspections.disable", async (data) => {
  const id = (data as { id: string }).id;
  const state = profiles();
  const profile = selectedProfile(state);
  if (!state || !profile) return;
  const next = profileOp(state, { op: "edit", name: profile.name, edit: { op: "setEnabled", ids: [id], enabled: false } });
  if ("error" in next) return;
  try {
    setProfiles(await inspectionsApi.save(next));
    showBalloon(`Inspection disabled in profile '${profile.name}'`, "Settings | Editor | Inspections turns it on again.");
  } catch (e) {
    showBalloon("Could not disable the inspection", e instanceof Error ? e.message : String(e));
  }
});
