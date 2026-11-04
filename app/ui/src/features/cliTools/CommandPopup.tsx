/**
 * Tools | Run Command… (Ctrl+Shift+X, `CommandLineToolsAction`): the Command Line Tools
 * Console popup. The input completes aliases, commands, options and arguments; Enter runs
 * the command. Its output goes to the Command Line Tools Console tool window, or stays in
 * the popup for tools whose "Show console in" is "Pop-up window".
 */
import { createSignal, For, Show } from "solid-js";
import { RefreshCw, RotateCcw, Settings, Square, SquareTerminal, X } from "lucide-solid";
import { Console } from "@/features/run/Console";
import { openSettings } from "@/features/settings/dialogState";
import { shortcutLabel } from "@/keymap";
import { toolName } from "@/ipc/cliTools";
import { ConsoleInput } from "./ConsoleInput";
import {
  enabledTools,
  findRun,
  isLive,
  prepareConsole,
  reloadAll,
  rerun,
  runInput,
  runLines,
  sendInput,
  statusText,
  stopRun,
  toolsLoaded,
} from "./store";
import { t as tr } from "@/i18n";

const [open, setOpen] = createSignal(false);
const [initial, setInitial] = createSignal("");
/** The run whose output the popup shows. */
const [shown, setShown] = createSignal<string | null>(null);

export { open as runCommandOpen };

export const SETTINGS_PAGE = "tools.command-line-tools";

export function openRunCommand(prefill = ""): void {
  setInitial(prefill);
  setShown(null);
  setOpen(true);
  void prepareConsole();
}

export function closeRunCommand(): void {
  setOpen(false);
}

export function RunCommandPopup() {
  const run = () => findRun(shown());
  let input: HTMLInputElement | undefined;
  const start = async (text: string) => {
    const r = await runInput(text);
    if (r?.target === "popup") setShown(r.id);
    else closeRunCommand();
    input?.focus();
  };
  return (
    <Show when={open()}>
      <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closeRunCommand()}>
        <div class="popup cli-popup" classList={{ "with-output": !!run() }} role="dialog" aria-label={tr("tools.cliTools.runCommand")}>
          <div class="cli-popup-header">
            <SquareTerminal size={15} class="cli-popup-icon" />
            <span class="cli-popup-title">{tr("tools.cliTools.runCommand")}</span>
            <span class="cli-popup-aliases">
              <For each={enabledTools()}>
                {(t) => (
                  <span class="cli-alias" title={`${toolName(t)}: ${t.path}`}>
                    <code>{t.alias}</code> {toolName(t)}
                  </span>
                )}
              </For>
            </span>
            <span class="kbd cli-popup-shortcut">{shortcutLabel("CommandLineToolsAction")}</span>
            <button class="icon-btn" title={tr("tools.cliTools.reloadCommandLists")} onClick={() => void reloadAll()}>
              <RefreshCw size={13} />
            </button>
            <button
              class="icon-btn"
              title={tr("tools.cliTools.commandLineToolSupportSettings")}
              onClick={() => {
                closeRunCommand();
                openSettings(SETTINGS_PAGE);
              }}
            >
              <Settings size={13} />
            </button>
            <button class="icon-btn" title={tr("common.closeEsc")} onClick={closeRunCommand}>
              <X size={14} />
            </button>
          </div>
          <Show when={toolsLoaded() && !enabledTools().length}>
            <div class="cli-popup-notice">
              {tr("tools.cliTools.noCommandLineToolsAre")}
              <button
                class="link-btn"
                onClick={() => {
                  closeRunCommand();
                  openSettings(SETTINGS_PAGE);
                }}
              >
                {tr("tools.cliTools.addAToolEllipsis")}
              </button>
            </div>
          </Show>
          <ConsoleInput
            initial={initial()}
            autofocus
            ref={(el) => (input = el)}
            running={isLive(run())}
            onStop={() => void stopRun(shown())}
            onRun={(text) => void start(text)}
            onEscape={closeRunCommand}
          />
          <Show when={run()}>
            {(r) => (
              <div class="cli-popup-output">
                <div class="cli-run-bar">
                  <code class="cli-run-cmdline" title={r().commandLine}>
                    {r().commandLine || r().input}
                  </code>
                  <span class="cli-run-status" classList={{ ok: r().exitCode === 0, failed: !isLive(r()) && r().exitCode !== 0 }}>
                    <Show when={isLive(r())}>
                      <span class="se-spinner" />
                    </Show>
                    {statusText(r())}
                  </span>
                  <Show
                    when={isLive(r())}
                    fallback={
                      <button class="icon-btn" title={tr("tools.cliTools.rerun")} onClick={() => void rerun(r().id)}>
                        <RotateCcw size={14} />
                      </button>
                    }
                  >
                    <button class="icon-btn" title={tr("tools.cliTools.stopCtrlC")} onClick={() => void stopRun(r().id)}>
                      <Square size={12} fill="currentColor" />
                    </button>
                  </Show>
                </div>
                <Console
                  lines={() => runLines(r().id)}
                  version={() => r().version}
                  softWrap={false}
                  scrollToEnd
                  onInput={isLive(r()) ? (t) => void sendInput(r().id, t) : undefined}
                />
              </div>
            )}
          </Show>
          <div class="cli-popup-footer muted">
            <span>{tr("tools.cliTools.tabToCompleteHistoryEnter")}</span>
          </div>
        </div>
      </div>
    </Show>
  );
}
