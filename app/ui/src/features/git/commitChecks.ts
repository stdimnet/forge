/**
 * Commit checks of the commit panel (the gear "Commit Options"), like PHPStorm's: before the
 * commit the changed files are reformatted, rearranged, their imports optimized and code
 * cleanup run (the Actions on Save building blocks), then the code is analyzed, TODO items in
 * the changed lines are listed and the chosen tests run. Problems ask "Review" or
 * "Commit Anyway". Settings: `vcs.commit.*` (Settings | Version Control | Commit).
 */
import { EditorState } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { askChoice } from "@/features/contextMenu/dialogs";
import { formattedText, replaceChangedPart } from "@/features/lsp/format";
import { diagnostics, onDiagnostics } from "@/features/lsp/store";
import { findConfiguration } from "@/features/run/configStore";
import { launch } from "@/features/run/runStore";
import { patterns } from "@/features/todo/store";
import { viewFor } from "@/editor/views";
import { lspApi, type EditorDiagnostic } from "@/ipc/lsp";
import { saveActionsApi } from "@/ipc/saveActions";
import { todoApi } from "@/ipc/workbench";
import { workspaceApi } from "@/ipc/workspace";
import { editor, saveFile } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { isPluginActive } from "@/state/project";
import { getSetting } from "@/state/settings";
import { changesApi } from "./changesApi";
import { reloadOpenFiles, notify } from "./store";
import { t as tr } from "@/i18n";

/** The checks and their settings keys, in the order PHPStorm runs them. */
export const COMMIT_CHECKS = [
  { key: "vcs.commit.reformat", label: tr("git.commitChecks.reformatCode") },
  { key: "vcs.commit.rearrange", label: tr("git.commitChecks.rearrangeCode") },
  { key: "vcs.commit.optimizeImports", label: tr("git.commitChecks.optimizeImports") },
  { key: "vcs.commit.analyze", label: tr("git.commitChecks.analyzeCode") },
  { key: "vcs.commit.checkTodo", label: tr("git.commitChecks.checkTodo") },
  { key: "vcs.commit.cleanup", label: tr("git.commitChecks.cleanup") },
] as const;

const on = (key: string) => getSetting<boolean>(key, false);

const textFile = (path: string) => !/\.(png|jpe?g|gif|webp|ico|pdf|zip|gz|jar|phar|woff2?|ttf|eot)$/i.test(path);

/** The text of a file as the editor has it. */
async function currentText(path: string): Promise<string | null> {
  const tab = editor.tabs.find((t) => t.path === path && !t.view);
  if (tab) return tab.content;
  return workspaceApi.readFile(path).catch(() => null);
}

/** Write a changed text back: through the open editor (then saved) or to the file. */
async function writeBack(path: string, before: string, after: string): Promise<boolean> {
  if (after === before) return false;
  const view = viewFor(path);
  if (view && view.state.doc.toString() === before) {
    replaceChangedPart(view, after, "commit.check");
    await saveFile(path);
  } else {
    await workspaceApi.writeFile(path, after);
    await reloadOpenFiles([path]);
  }
  return true;
}

/** Reformat Code for a file that may not be open: a detached state stands in for the view. */
async function reformatted(path: string, text: string): Promise<string> {
  const view = viewFor(path);
  if (view && view.state.doc.toString() === text) return formattedText(path, view);
  await lspApi.didOpen(path, text).catch(() => undefined);
  try {
    return await formattedText(path, { state: EditorState.create({ doc: text }) } as EditorView);
  } finally {
    await lspApi.didClose(path).catch(() => undefined);
  }
}

/** Reformat / Rearrange / Optimize imports / Cleanup on the committed files. */
async function modifyCode(paths: string[]): Promise<number> {
  const reformat = on("vcs.commit.reformat");
  const php = {
    optimizeImports: on("vcs.commit.optimizeImports"),
    rearrange: on("vcs.commit.rearrange"),
    codeCleanup: on("vcs.commit.cleanup"),
  };
  const phpActive = (php.optimizeImports || php.rearrange || php.codeCleanup) && isPluginActive("forge.php");
  if (!reformat && !phpActive) return 0;
  let changed = 0;
  for (const path of paths.filter(textFile)) {
    const before = await currentText(path);
    if (before === null) continue;
    let text = before;
    try {
      if (phpActive && path.endsWith(".php")) text = (await saveActionsApi.php(text, php)).text;
      if (reformat) text = await reformatted(path, text);
      if (await writeBack(path, before, text)) changed++;
    } catch (e) {
      console.warn("commit check failed for", path, e);
    }
  }
  return changed;
}

/** Diagnostics of the files, waiting a moment for language servers to report. */
async function collectProblems(paths: string[]): Promise<{ errors: number; warnings: number; files: string[] }> {
  const opened: string[] = [];
  const pending = new Set(paths.filter((p) => !diagnostics[p]));
  const reported = new Promise<void>((resolve) => {
    if (!pending.size) return resolve();
    const off = onDiagnostics((p) => {
      pending.delete(p);
      if (!pending.size) {
        off();
        resolve();
      }
    });
    setTimeout(() => {
      off();
      resolve();
    }, 2500);
  });
  for (const p of pending) {
    if (viewFor(p)) continue;
    const text = await currentText(p);
    if (text === null) continue;
    await lspApi.didOpen(p, text).catch(() => undefined);
    opened.push(p);
  }
  await reported;
  let errors = 0;
  let warnings = 0;
  const files: string[] = [];
  for (const p of paths) {
    const list: EditorDiagnostic[] = diagnostics[p] ?? [];
    const e = list.filter((d) => d.severity === "error").length;
    const w = list.filter((d) => d.severity === "warning").length;
    errors += e;
    warnings += w;
    if (e || w) files.push(p);
  }
  for (const p of opened) await lspApi.didClose(p).catch(() => undefined);
  return { errors, warnings, files };
}

/** TODO items in the added or changed lines of the files. */
async function changedTodos(paths: string[]): Promise<number> {
  const res = await todoApi.scan({ patterns: patterns(), paths: paths.filter(textFile) }).catch(() => null);
  if (!res) return 0;
  let n = 0;
  for (const f of res.files) {
    const hunks = await changesApi.fileHunks(f.path).catch(() => []);
    // TODO lines are 0-based like the hunks.
    n += f.items.filter((i) => hunks.some((h) => i.line >= h.newStart && i.line < h.newStart + h.newLen)).length;
  }
  return n;
}

const plural = (n: number, what: string) => `${n} ${what}${n === 1 ? "" : "s"}`;

/**
 * Run the enabled checks on the files about to be committed. Returns false when the user
 * chose to review the problems instead of committing.
 */
export async function runCommitChecks(paths: string[], onProgress: (text: string) => void): Promise<boolean> {
  const files = paths.filter((p) => editor.tabs.some((t) => t.path === p) || textFile(p));
  if (on("vcs.commit.reformat") || on("vcs.commit.optimizeImports") || on("vcs.commit.rearrange") || on("vcs.commit.cleanup")) {
    onProgress("Running code modifications…");
    const n = await modifyCode(files);
    if (n) notify("info", tr("git.commitChecks.commitChecks"), tr("tools.git.changedByTheCodeModifications", { value: plural(n, "file") }));
  }
  if (on("vcs.commit.analyze")) {
    onProgress("Analyzing code…");
    const p = await collectProblems(files);
    if (p.errors || p.warnings) {
      const choice = await askChoice({
        title: tr("git.commitChecks.codeAnalysis"),
        message: `${plural(p.errors, "error")} and ${plural(p.warnings, "warning")} found in ${plural(p.files.length, "file")}.`,
        choices: [
          { id: "review", label: tr("git.commitChecks.review") },
          { id: "commit", label: tr("git.commitChecks.commitAnyway") },
        ],
      });
      if (choice !== "commit") {
        if (choice === "review") showToolWindow("problems");
        return false;
      }
    }
  }
  if (on("vcs.commit.checkTodo")) {
    onProgress("Checking TODO…");
    const n = await changedTodos(files);
    if (n) {
      const choice = await askChoice({
        title: "TODO",
        message: `${plural(n, "TODO item")} found in the changed code.`,
        choices: [
          { id: "review", label: tr("git.commitChecks.review") },
          { id: "commit", label: tr("git.commitChecks.commitAnyway") },
        ],
      });
      if (choice !== "commit") {
        if (choice === "review") showToolWindow("todo");
        return false;
      }
    }
  }
  if (on("vcs.commit.runTests")) {
    const name = getSetting<string>("vcs.commit.testConfiguration", "");
    const cfg = findConfiguration(name);
    if (!cfg) {
      notify("error", tr("git.commitChecks.runTests"), name ? tr("git.commitChecks.runConfigurationNameNotFound", { name }) : tr("git.commitChecks.chooseARunConfigurationIn"));
      return false;
    }
    onProgress(tr("git.commitChecks.runningName", { name: cfg.name }));
    const handle = await launch(cfg);
    const code = handle ? await handle.exited : null;
    if (code !== 0) {
      const choice = await askChoice({
        title: tr("git.commitChecks.testsFailed"),
        message: handle ? `${cfg.name} finished with exit code ${code ?? "?"}.` : `${cfg.name} could not be started.`,
        choices: [
          { id: "review", label: tr("git.commitChecks.review") },
          { id: "commit", label: tr("git.commitChecks.commitAnyway") },
        ],
      });
      if (choice !== "commit") {
        if (choice === "review") showToolWindow("run");
        return false;
      }
    }
  }
  return true;
}
