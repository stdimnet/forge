/**
 * Vim emulation (plugin `forge.vim`), following PHPStorm's IdeaVim: Tools | Vim
 * (`VimPluginToggle`), the mode widget in the status bar, Settings | Editor | Vim with the
 * shortcut conflict table, and `~/.ideavimrc`. The engine is Rust (`crates/forge-vim`), run
 * synchronously as WebAssembly; `extension.ts` connects it to CodeMirror.
 */
import { createEffect, createRoot, createSignal, on, Show } from "solid-js";
import type { EditorView } from "@codemirror/view";
import type { VimHandlerOverride, VimMode, VimRcReport, VimResponse } from "@/ipc/vim";
import { vimApi } from "@/ipc/vim";
import type { SettingObject } from "@/ipc/settings";
import { registerEditorExtension } from "@/editor/extensions";
import { activeView } from "@/editor/views";
import { registerMainMenuToggle } from "@/features/mainMenu/model";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerObjectSettingEditor, SchemaPage } from "@/features/settings/SchemaPage";
import { openSettings } from "@/features/settings/dialogState";
import { registerStatusItem } from "@/features/statusItems";
import { onCopy, recordCopy } from "@/features/clipboard/clipboard";
import { isActionAvailable, registerKeymapActions, runAction, showBalloon } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { openFile } from "@/state/editor";
import { isPluginActive } from "@/state/project";
import { getSetting, saveSetting } from "@/state/settings";
import { loadVimEngine, vimEngine, vimReady } from "./engine";
import { refreshVimEditors, setVimHost, vimExtension } from "./extension";
import { HandlersEditor } from "./HandlersEditor";
import "./vim.css";

export const VIM_ENABLED_KEY = "vim.enabled";
const HANDLERS_KEY = "vim.shortcutHandlers";
const RC_KEY = "vim.rcFile";

/** Whether Vim emulation is on (plugin enabled and Tools | Vim checked). */
export const vimEnabled = (): boolean => isPluginActive("forge.vim") && getSetting<boolean>(VIM_ENABLED_KEY, false);

// ---- Status of the focused editor

interface VimStatus {
  mode: VimMode;
  showcmd: string;
  recording: string | null;
  message: { text: string; error: boolean } | null;
}

const [status, setStatus] = createSignal<VimStatus>({ mode: "normal", showcmd: "", recording: null, message: null });
let messageTimer: ReturnType<typeof setTimeout> | undefined;

function onResponse(view: EditorView, res: VimResponse): void {
  if (!view.hasFocus && activeView() !== view) return;
  const message = res.message;
  if (message && message.text.includes("\n")) {
    // Listings (:registers, :marks, :map) are too long for the status bar.
    const [title, ...rest] = message.text.split("\n");
    showBalloon(title, rest.join("\n"), 10000, undefined, { log: false });
  }
  setStatus((s) => ({
    mode: res.mode,
    showcmd: res.showcmd,
    recording: res.recording,
    message: message && !message.text.includes("\n") ? message : s.message,
  }));
  if (message && !message.text.includes("\n")) {
    clearTimeout(messageTimer);
    messageTimer = setTimeout(() => setStatus((s) => ({ ...s, message: null })), message.error ? 6000 : 4000);
  }
}

export const MODE_LABELS: Record<VimMode, string> = {
  normal: "NORMAL",
  insert: "INSERT",
  replace: "REPLACE",
  visual: "VISUAL",
  visualLine: "VISUAL LINE",
  visualBlock: "VISUAL BLOCK",
};

function VimStatusWidget() {
  return (
    <button
      class="status-item vim-status"
      data-mode={status().mode}
      title="Vim emulation. Click to configure it in Settings | Editor | Vim."
      onClick={() => openSettings("editor.vim")}
    >
      <span class="vim-status-mode">{MODE_LABELS[status().mode]}</span>
      <Show when={status().recording}>
        <span class="vim-status-recording">recording @{status().recording}</span>
      </Show>
      <Show when={status().showcmd}>
        <span class="vim-status-showcmd">{status().showcmd}</span>
      </Show>
      <Show when={status().message}>
        {(m) => (
          <span class="vim-status-message" classList={{ error: m().error }}>
            {m().text}
          </span>
        )}
      </Show>
    </button>
  );
}

registerStatusItem({ id: "vim.mode", align: "right", order: -10, component: VimStatusWidget, when: vimEnabled });

// ---- ~/.ideavimrc

const [rcState, setRcState] = createSignal<{ path: string | null; report: VimRcReport } | null>(null);

/** Read and apply ~/.ideavimrc; `notify` reports the result in a balloon. */
export async function reloadVimRc(notify = false): Promise<void> {
  if (!vimReady()) return;
  let rc;
  try {
    rc = await vimApi.readRc(getSetting<string>(RC_KEY, ""));
  } catch (e) {
    showBalloon("Cannot read .ideavimrc", String(e), 6000, undefined, { type: "error" });
    return;
  }
  const report = vimEngine.loadRc(rc.text);
  setRcState({ path: rc.path, report });
  refreshVimEditors();
  const problems = report.problems.map((p) => `Line ${p.line}: ${p.message}`).join("\n");
  if (report.problems.length) {
    showBalloon(
      `${rc.path ?? ".ideavimrc"}: ${report.problems.length} line${report.problems.length === 1 ? "" : "s"} ignored`,
      problems,
      8000,
      [{ label: "Vim Settings", run: () => openSettings("editor.vim") }],
      { type: "warning" },
    );
  } else if (notify) {
    showBalloon(rc.path ? `Reloaded ${rc.path}` : "No .ideavimrc found", undefined, 3000, undefined, { log: false });
  }
}

function RcStatus() {
  return (
    <div class="vim-rc-status" data-testid="vim-rc-status">
      <Show when={rcState()} fallback={<span>The configuration file is read when Vim emulation is turned on.</span>}>
        {(s) => (
          <>
            <span>
              {s().path
                ? `${s().path}: ${s().report.applied} command${s().report.applied === 1 ? "" : "s"} applied`
                : "No ~/.ideavimrc found."}
              {s().report.problems.length ? `, ${s().report.problems.length} ignored` : ""}
            </span>
            <button class="btn-secondary" onClick={() => void reloadVimRc(true)}>
              Reload
            </button>
          </>
        )}
      </Show>
    </div>
  );
}

// ---- Wiring

setVimHost({
  enabled: vimEnabled,
  runAction,
  isActionAvailable,
  openFile: async (path) => {
    const rel = path.replace(/^\.\//, "");
    try {
      await openFile(rel);
    } catch (e) {
      showBalloon(`Cannot open ${path}`, String(e), 4000, undefined, { type: "error" });
    }
  },
  reloadRc: () => reloadVimRc(true),
  copy: (text) => {
    void navigator.clipboard?.writeText(text).catch(() => undefined);
    recordCopy(text);
  },
  status: onResponse,
});

registerEditorExtension(() => vimExtension());
onCopy((text) => vimEngine.setClipboard(text));
void loadVimEngine();

createRoot(() => {
  createEffect(() => {
    const on = vimEnabled();
    const handlers = (getSetting<SettingObject[]>(HANDLERS_KEY, []) ?? []) as unknown as VimHandlerOverride[];
    if (!vimReady()) return;
    vimEngine.configure(on, Array.isArray(handlers) ? handlers : []);
    refreshVimEditors();
  });
  createEffect(
    on([vimEnabled, vimReady, () => getSetting<string>(RC_KEY, "")], ([on, ready]) => {
      if (on && ready) void reloadVimRc();
    }),
  );
});

async function toggleVim(): Promise<void> {
  const next = !vimEnabled();
  await saveSetting(VIM_ENABLED_KEY, next);
  activeView()?.focus();
}

registerKeymapActions([
  {
    id: "VimPluginToggle",
    title: "Vim",
    group: "Plugins/Vim",
    shortcuts: { $default: ["Ctrl+Alt+V"], macOS: ["Cmd+Alt+V"] },
  },
  { id: "IdeaVim.ReloadVimRc", title: "Reload .ideavimrc", group: "Plugins/Vim", shortcuts: { $default: [], macOS: [] } },
]);
registerCommand({ id: "VimPluginToggle", title: "Vim", category: "Tools", run: toggleVim });
registerCommand({ id: "IdeaVim.ReloadVimRc", title: "Reload .ideavimrc", category: "Tools", run: () => reloadVimRc(true) });
registerMainMenuToggle("VimPluginToggle", vimEnabled);

registerSettingsPage({
  id: "editor.vim",
  title: "Vim",
  group: "Editor",
  order: 70,
  keywords: ["ideavim", "ideavimrc", "vim emulation", "sethandler", "shortcut conflicts", "handler"],
  component: () => <SchemaPage pageId="editor.vim" sectionHeader={(s) => (s === "Emulation" ? <RcStatus /> : null)} />,
});
registerObjectSettingEditor(HANDLERS_KEY, HandlersEditor);
