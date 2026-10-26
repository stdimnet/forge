/**
 * PyForge in the editor: requirements inspections (unsatisfied requirements in
 * requirements files and pyproject.toml, imports missing from the requirements) with
 * their quick fixes (Alt+Enter), completion of package names and versions in dependency
 * files, ▶ on `if __name__ == "__main__":` and Reformat Code with ruff format / Black.
 * The problems and contexts are computed in Rust (`python_inspect`, `python_complete`).
 */
import { createSignal } from "solid-js";
import { EditorState, type Extension } from "@codemirror/state";
import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { Play, Settings2 } from "lucide-solid";
import { pythonApi, type PythonFix, type PythonProblem } from "@/ipc/python";
import type { ContextAction } from "@/ipc/codeintel";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { registerEditorExtension } from "@/editor/extensions";
import { registerLineMarkerProvider } from "@/editor/lineMarkers";
import { viewFor } from "@/editor/views";
import { registerContextActionHandler, registerContextActionProvider } from "@/features/codeintel/intentions";
import { publishDiagnostics } from "@/features/lsp/store";
import { registerFormatProvider, replaceChangedPart } from "@/features/lsp/format";
import { openMenu } from "@/features/run/PopupMenu";
import { useTemporary } from "@/features/run/configStore";
import { launch } from "@/features/run/runStore";
import { openRunConfigurations } from "@/features/run/dialogState";
import { workspaceApi } from "@/ipc/workspace";
import { shortcutLabel, showBalloon } from "@/keymap";
import { commands, registerCommand } from "@/state/commands";
import { activeTab } from "@/state/editor";
import { getSetting, saveSettings } from "@/state/settings";
import { packageAction, packagesRevision, pythonActive } from "./state";

const SOURCE = "pyforge";

export const isPythonPath = (path: string) => /\.pyw?$/.test(path);
const fileName = (path: string) => path.replace(/\\/g, "/").split("/").pop() ?? path;
export const isRequirementsPath = (path: string) => {
  const p = path.replace(/\\/g, "/");
  const name = fileName(p);
  return /\.(txt|in)$/.test(name) && (/^(requirements|constraints)/.test(name) || p.includes("requirements/"));
};
export const isPyproject = (path: string) => fileName(path) === "pyproject.toml";
const inspected = (path: string) => isPythonPath(path) || isRequirementsPath(path) || isPyproject(path);

/** The problems last published per file (for Alt+Enter). */
const [problems, setProblems] = createSignal<Record<string, PythonProblem[]>>({});
export { problems as pythonProblems };

function publish(path: string, list: PythonProblem[]): void {
  setProblems((prev) => ({ ...prev, [path]: list }));
  const diags: EditorDiagnostic[] = list.map((p) => ({
    range: { start: { line: p.line, character: p.start }, end: { line: p.line, character: p.end } },
    severity: p.severity,
    message: p.message,
    source: "Python",
    code: p.code,
  }));
  publishDiagnostics(path, diags, SOURCE);
}

// ---- Inspections and run markers ----------------------------------------------------------

const mainConfiguration = (path: string) => ({
  name: fileName(path).replace(/\.pyw?$/, ""),
  type: "python",
  options: { target: "script", script: path },
});

function runFile(path: string): void {
  void launch(useTemporary(mainConfiguration(path)));
}

registerLineMarkerProvider({
  appliesTo: inspected,
  watch: () => [pythonActive(), packagesRevision(), getSetting<string[]>("python.inspections.ignoredPackages", [])],
  markers: async (path, text) => {
    if (!pythonActive()) {
      publish(path, []);
      return [];
    }
    const [found, mains] = await Promise.all([
      pythonApi.inspect(path, text).catch(() => [] as PythonProblem[]),
      isPythonPath(path) ? pythonApi.runMarkers(text).catch(() => []) : Promise.resolve([]),
    ]);
    publish(path, found);
    const name = fileName(path).replace(/\.pyw?$/, "");
    return mains.map((m) => ({
      line: m.line,
      icon: Play,
      title: `Run '${name}'`,
      className: "marker-run",
      onClick: (_view, e) =>
        openMenu({
          x: e.clientX,
          y: e.clientY + 6,
          items: [
            { label: `Run '${name}'`, icon: Play, iconColor: "var(--success)", shortcut: shortcutLabel("RunClass"), run: () => runFile(path) },
            { separator: true },
            { label: "Modify Run Configuration…", icon: Settings2, run: () => openRunConfigurations(useTemporary(mainConfiguration(path)).name) },
          ],
        }),
    }));
  },
});

// Ctrl+Shift+F10 in a Python file runs it.
const runClass = commands().get("RunClass");
if (runClass) {
  registerCommand({
    ...runClass,
    run: async () => {
      const tab = activeTab();
      if (tab && isPythonPath(tab.path) && pythonActive()) return runFile(tab.path);
      return runClass.run();
    },
  });
}

// ---- Quick fixes ---------------------------------------------------------------------------

/** "Add requirement": edit the dependency file (in its editor when it is open). */
export async function addRequirement(requirement: string, target: string): Promise<void> {
  const view = viewFor(target);
  try {
    const edit = await pythonApi.addRequirement(requirement, target, view?.state.doc.toString());
    if (view) replaceChangedPart(view, edit.text, "input");
    else await workspaceApi.writeFile(edit.path, edit.text);
    showBalloon(`Added '${requirement}' to ${fileName(target)}`);
  } catch (e) {
    showBalloon("Add requirement", String(e), 5000, undefined, { type: "error" });
  }
}

async function applyFix(fix: PythonFix): Promise<void> {
  switch (fix.kind) {
    case "installRequirements":
      await packageAction("install", fix.requirements);
      break;
    case "addRequirement":
      await addRequirement(fix.requirement, fix.target);
      break;
    case "ignore": {
      const list = getSetting<string[]>("python.inspections.ignoredPackages", []) ?? [];
      if (!list.includes(fix.package)) await saveSettings({ "python.inspections.ignoredPackages": [...list, fix.package] });
      break;
    }
  }
}

registerContextActionProvider(async (path, view) => {
  if (!inspected(path) || !pythonActive()) return [];
  const pos = view.state.selection.main.head;
  const line = view.state.doc.lineAt(pos);
  const col = pos - line.from;
  const here = (problems()[path] ?? []).filter((p) => p.line === line.number - 1 && col >= p.start && col <= p.end);
  const list = here.length ? here : (problems()[path] ?? []).filter((p) => p.line === line.number - 1);
  return list.flatMap((p) =>
    p.fixes.map(
      (fix, i): ContextAction => ({
        title: fix.title,
        kind: fix.kind === "ignore" ? "intention" : "quickFix",
        source: "python",
        isPreferred: i === 0,
        needsResolve: false,
        action: "Python.ApplyFix",
        data: fix,
      }),
    ),
  );
});

registerContextActionHandler("Python.ApplyFix", (data) => void applyFix(data as PythonFix));

// ---- Completion in dependency files -------------------------------------------------------

function completionSource(path: string) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!pythonActive()) return null;
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    if (!ctx.explicit && !/[\w.=<>~!-]$/.test(before)) return null;
    const answer = await pythonApi.complete(path, ctx.state.doc.toString(), ctx.pos).catch(() => null);
    if (!answer || ctx.aborted || !answer.items.length) return null;
    return {
      from: answer.from,
      options: answer.items.map((item, i) => ({
        label: item.label,
        detail: item.detail,
        type: item.kind === "package" ? "module" : "constant",
        boost: item.kind === "version" ? -i : undefined,
      })),
      filter: answer.items[0]?.kind !== "version",
      validFor: /^[\w.-]*$/,
    };
  };
}

registerEditorExtension(({ path }): Extension | null => {
  if (!isRequirementsPath(path) && !isPyproject(path)) return null;
  const data = [{ autocomplete: completionSource(path) }];
  return EditorState.languageData.of(() => data);
});

// ---- Reformat Code ---------------------------------------------------------------------

registerFormatProvider(async (path, text) => {
  if (!isPythonPath(path) && !path.endsWith(".pyi")) return null;
  const formatter = getSetting<string>("python.formatter", "ruff");
  if (formatter === "none") return text;
  if (formatter === "lsp") return null;
  try {
    return await pythonApi.format(path, text);
  } catch (e) {
    showBalloon(formatter === "black" ? "Black" : "ruff format", String(e), 5000, undefined, { type: "error" });
    return text;
  }
});
