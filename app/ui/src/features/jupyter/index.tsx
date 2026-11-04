/**
 * Jupyter notebooks (PyCharm's Jupyter support, plugin `forge.jupyter`): `.ipynb` files open
 * in the notebook editor, diffs of notebooks compare cell by cell, and the notebook actions
 * with Jupyter's shortcuts live in the keymap (command mode: `notebook-command`; command
 * and edit mode: `notebook`). Settings | Languages & Frameworks | Jupyter.
 */
import { createSignal, Show } from "solid-js";
import { registerFileEditor } from "@/shell/editorViews";
import { registerCommand } from "@/state/commands";
import { activeTab } from "@/state/editor";
import { registerKeymapActions } from "@/keymap/store";
import type { ActionContext } from "@/ipc/keymap";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { registerDiffRenderer } from "@/features/git/diffRenderers";
import { getSetting } from "@/state/settings";
import { isPluginActive } from "@/state/project";
import { jupyterApi } from "@/ipc/jupyter";
import { closeCompletion, completionStatus } from "@codemirror/autocomplete";
import { NotebookEditor } from "./NotebookEditor";
import { NotebookDiffView } from "./NotebookDiffView";
import { existingNotebook, type NotebookController } from "./store";
import "./jupyter.css";
import { t } from "@/i18n";

export const isNotebookPath = (path: string): boolean => /\.ipynb$/i.test(path) && !path.includes("://");
const enabled = () => isPluginActive("forge.jupyter");

registerFileEditor({ id: "jupyter", accepts: (p) => isNotebookPath(p) && enabled(), component: NotebookEditor });

registerDiffRenderer({
  id: "jupyter",
  title: "Cells",
  accepts: (p) => isNotebookPath(p) && enabled(),
  preferred: () => getSetting<string>("jupyter.diff.view", "cells") !== "json",
  component: NotebookDiffView,
});

/** The notebook of the active editor tab. */
const active = (): NotebookController | undefined => existingNotebook(activeTab()?.path);

/** Escape in a cell editor: close completion first, then go to command mode. */
function escape(nb: NotebookController): void {
  const cell = nb.state.cells[nb.state.selected];
  const view = cell ? nb.views.get(cell.id) : undefined;
  if (view && completionStatus(view.state)) {
    closeCompletion(view);
    return;
  }
  nb.enterCommand();
}

interface NotebookAction {
  id: string;
  title: string;
  context: ActionContext;
  keys?: string[];
  mac?: string[];
  run: (nb: NotebookController) => void | Promise<void>;
}

const ACTIONS: NotebookAction[] = [
  { id: "Jupyter.RunCellSelectBelow", title: "Run Cell and Select Below", context: "notebook", keys: ["Shift+Enter"], run: (nb) => nb.run(nb.state.selected, "next") },
  { id: "Jupyter.RunCell", title: "Run Cell", context: "notebook", keys: ["Ctrl+Enter"], mac: ["Cmd+Enter"], run: (nb) => nb.run(nb.state.selected, "stay") },
  { id: "Jupyter.RunCellInsertBelow", title: "Run Cell and Insert Below", context: "notebook", keys: ["Alt+Enter"], mac: ["Opt+Enter"], run: (nb) => nb.run(nb.state.selected, "insert") },
  { id: "Jupyter.RunAll", title: "Run All Cells", context: "notebook", keys: ["Ctrl+Alt+Shift+Enter"], mac: ["Cmd+Opt+Shift+Enter"], run: (nb) => nb.runAll() },
  { id: "Jupyter.InterruptKernel", title: "Interrupt Kernel", context: "notebook-command", keys: ["I I"], run: (nb) => nb.interrupt() },
  { id: "Jupyter.RestartKernel", title: "Restart Kernel", context: "notebook-command", keys: ["0 0"], run: (nb) => nb.restart() },
  { id: "Jupyter.ShutdownKernel", title: "Shut Down Kernel", context: "any", run: (nb) => nb.shutdown() },
  { id: "Jupyter.EnterCommandMode", title: "Enter Command Mode", context: "notebook", keys: ["Escape"], run: escape },
  { id: "Jupyter.EnterEditMode", title: "Enter Edit Mode", context: "notebook-command", keys: ["Enter"], run: (nb) => nb.enterEdit() },
  { id: "Jupyter.InsertCellAbove", title: "Insert Cell Above", context: "notebook-command", keys: ["A"], run: (nb) => nb.insert(nb.state.selected, "code", false) },
  { id: "Jupyter.InsertCellBelow", title: "Insert Cell Below", context: "notebook-command", keys: ["B"], run: (nb) => nb.insert(nb.state.selected + 1, "code", false) },
  { id: "Jupyter.DeleteCell", title: "Delete Cell", context: "notebook-command", keys: ["D D"], run: (nb) => nb.remove() },
  { id: "Jupyter.ToCode", title: "Change Cell Type to Code", context: "notebook-command", keys: ["Y"], run: (nb) => nb.setType("code") },
  { id: "Jupyter.ToMarkdown", title: "Change Cell Type to Markdown", context: "notebook-command", keys: ["M"], run: (nb) => nb.setType("markdown") },
  { id: "Jupyter.ToRaw", title: "Change Cell Type to Raw", context: "notebook-command", keys: ["R"], run: (nb) => nb.setType("raw") },
  { id: "Jupyter.SelectCellAbove", title: "Select Cell Above", context: "notebook-command", keys: ["Up", "K"], run: (nb) => nb.select(nb.state.selected - 1) },
  { id: "Jupyter.SelectCellBelow", title: "Select Cell Below", context: "notebook-command", keys: ["Down", "J"], run: (nb) => nb.select(nb.state.selected + 1) },
  { id: "Jupyter.MoveCellUp", title: "Move Cell Up", context: "notebook-command", keys: ["Ctrl+Shift+Up"], mac: ["Cmd+Shift+Up"], run: (nb) => nb.move(-1) },
  { id: "Jupyter.MoveCellDown", title: "Move Cell Down", context: "notebook-command", keys: ["Ctrl+Shift+Down"], mac: ["Cmd+Shift+Down"], run: (nb) => nb.move(1) },
  { id: "Jupyter.ClearAllOutputs", title: "Clear All Outputs", context: "any", run: (nb) => nb.clearOutputs() },
  { id: "Jupyter.ShowVariables", title: "Show Variables", context: "any", run: (nb) => nb.toggleVariables() },
];

registerKeymapActions(
  ACTIONS.map((a) => ({
    id: a.id,
    title: a.title,
    group: "Jupyter",
    context: a.context,
    shortcuts: a.keys ? { $default: a.keys, macOS: a.mac ?? a.keys } : undefined,
  })),
);
for (const a of ACTIONS) {
  registerCommand({
    id: a.id,
    title: a.title,
    category: "Jupyter",
    run: () => {
      const nb = active();
      if (nb) return a.run(nb);
    },
  });
}

/** The token of the configured server, stored in the OS keychain through Rust. */
function ServerToken() {
  const [token, setToken] = createSignal("");
  const [result, setResult] = createSignal<{ ok: boolean; text: string } | null>(null);
  const url = () => getSetting<string>("jupyter.server.url", "").trim();
  const test = async () => {
    setResult(null);
    try {
      const check = await jupyterApi.serverTest(url(), token() || undefined);
      setToken("");
      const kernels = check.kernels.map((k) => k.displayName).join(", ");
      setResult({ ok: true, text: `Connected to ${check.base}. Kernels: ${kernels || "none"}` });
    } catch (e) {
      setResult({ ok: false, text: String(e).replace(/^Error: /, "") });
    }
  };
  return (
    <div class="nb-settings-token">
      <h3 class="set-section-title">Server Token</h3>
      <p class="muted">Stored in the operating system's keychain, never in settings files. Apply the URL above first.</p>
      <div class="nb-settings-row">
        <input
          type="password"
          class="input"
          placeholder="Token (or paste a URL with ?token= above)"
          value={token()}
          onInput={(e) => setToken(e.currentTarget.value)}
          aria-label="Jupyter server token"
        />
        <button class="pill-btn" disabled={!url()} onClick={() => void test()}>
          Test Connection
        </button>
        <button class="pill-btn" disabled={!url()} onClick={() => void jupyterApi.serverForgetToken(url()).then(() => setResult({ ok: true, text: "Token removed" }))}>
          Forget Token
        </button>
      </div>
      <Show when={result()}>{(r) => <div class={r().ok ? "nb-settings-ok" : "nb-settings-error"}>{r().text}</div>}</Show>
    </div>
  );
}

registerSettingsPage({
  id: "lang.jupyter",
  title: t("settings.page.lang.jupyter"),
  group: "Languages & Frameworks",
  order: 26,
  keywords: ["jupyter", "notebook", "ipynb", "kernel", "ipykernel", "jupyter server", "token", "dataframe", "pandas"],
  component: () => (
    <div class="py-settings-page">
      <SchemaPage pageId="lang.jupyter" />
      <ServerToken />
    </div>
  ),
});
