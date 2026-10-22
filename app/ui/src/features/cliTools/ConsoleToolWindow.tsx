/**
 * The Command Line Tools Console tool window: a tab per command (status, Rerun, Stop,
 * Clear, Close), the console with clickable file paths and the input line with completion
 * at the bottom.
 */
import { createSignal, For, onMount, Show } from "solid-js";
import { ArrowDownToLine, RefreshCw, RotateCcw, Settings, Square, SquareTerminal, Trash2, WrapText, X } from "lucide-solid";
import { Console } from "@/features/run/Console";
import { openSettings } from "@/features/settings/dialogState";
import { toolName } from "@/ipc/cliTools";
import { shortcutLabel } from "@/keymap";
import { ConsoleInput } from "./ConsoleInput";
import { SETTINGS_PAGE } from "./CommandPopup";
import {
  activeRun,
  activeRunId,
  clearRun,
  closeRun,
  enabledTools,
  isLive,
  prepareConsole,
  reloadAll,
  rerun,
  runInput,
  runLines,
  sendInput,
  setActiveRun,
  statusText,
  stopRun,
  toolWindowRuns,
  type ConsoleRun,
} from "./store";

/** Tabs in the tool window header. */
export function ConsoleTabs() {
  return (
    <div class="run-tabs" role="tablist">
      <For each={toolWindowRuns()}>
        {(r) => (
          <div
            class="run-tab"
            classList={{ active: activeRunId() === r.id }}
            role="tab"
            data-cli-tab={r.input}
            title={`${r.commandLine || r.input}${isLive(r) ? "" : ` (${statusText(r)})`}`}
            onClick={() => setActiveRun(r.id)}
            onMouseDown={(e) => e.button === 1 && (e.preventDefault(), void closeRun(r.id))}
          >
            <span class="run-tab-icon">
              <SquareTerminal size={14} />
              <Show when={isLive(r)}>
                <span class="run-tab-live" />
              </Show>
            </span>
            <span class="run-tab-title">{r.input}</span>
            <button
              class="run-tab-close"
              title="Close"
              onClick={(e) => {
                e.stopPropagation();
                void closeRun(r.id);
              }}
            >
              <X size={12} />
            </button>
          </div>
        )}
      </For>
    </div>
  );
}

/** Header buttons. */
export function ConsoleActions() {
  return (
    <>
      <button class="icon-btn" title="Reload Command Lists" onClick={() => void reloadAll()}>
        <RefreshCw size={14} />
      </button>
      <button class="icon-btn" title="Command Line Tool Support Settings" onClick={() => openSettings(SETTINGS_PAGE)}>
        <Settings size={14} />
      </button>
    </>
  );
}

function Welcome() {
  return (
    <div class="cli-welcome muted">
      <p>
        Type a command as <code>alias command [arguments]</code>, e.g. <code>c install</code> or <code>a make:model Order -m</code>.
        Tab completes, ↑↓ browse the history.
      </p>
      <Show
        when={enabledTools().length}
        fallback={
          <p>
            No command line tools are configured.{" "}
            <button class="link-btn" onClick={() => openSettings(SETTINGS_PAGE)}>
              Add a tool…
            </button>
          </p>
        }
      >
        <table class="cli-welcome-tools">
          <tbody>
            <For each={enabledTools()}>
              {(t) => (
                <tr>
                  <td>
                    <code>{t.alias}</code>
                  </td>
                  <td>{toolName(t)}</td>
                  <td class="muted">{t.path}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </Show>
      <p>
        Open the console from anywhere with <b>{shortcutLabel("CommandLineToolsAction") || "Ctrl+Shift+X"}</b>.
      </p>
    </div>
  );
}

function RunView(props: { run: ConsoleRun; scrollToEnd: boolean; softWrap: boolean }) {
  return (
    <Console
      lines={() => runLines(props.run.id)}
      version={() => props.run.version}
      softWrap={props.softWrap}
      scrollToEnd={props.scrollToEnd}
      onInput={isLive(props.run) ? (t) => void sendInput(props.run.id, t) : undefined}
    />
  );
}

export function ConsoleToolWindow() {
  const [scrollToEnd, setScrollToEnd] = createSignal(true);
  const [softWrap, setSoftWrap] = createSignal(false);
  let input: HTMLInputElement | undefined;
  onMount(() => void prepareConsole());
  const current = () => (activeRun()?.target === "toolWindow" ? activeRun() : toolWindowRuns().at(-1));
  return (
    <div class="run-window cli-window" data-testid="cli-console">
      <div class="run-session">
        <div class="run-toolbar" role="toolbar" aria-orientation="vertical">
          <button class="icon-btn run-tool" title="Rerun" disabled={!current()} onClick={() => void rerun(current()?.id)}>
            <RotateCcw size={15} />
          </button>
          <button
            class="icon-btn run-tool run-stop"
            title="Stop (Ctrl+C)"
            disabled={!isLive(current())}
            onClick={() => void stopRun(current()?.id)}
          >
            <Square size={13} fill="currentColor" />
          </button>
          <div class="run-tool-sep" />
          <button class="icon-btn run-tool" classList={{ active: scrollToEnd() }} title="Scroll to the End" onClick={() => setScrollToEnd((v) => !v)}>
            <ArrowDownToLine size={15} />
          </button>
          <button class="icon-btn run-tool" classList={{ active: softWrap() }} title="Soft-Wrap" onClick={() => setSoftWrap((v) => !v)}>
            <WrapText size={15} />
          </button>
          <button class="icon-btn run-tool" title="Clear All" disabled={!current()} onClick={() => clearRun(current()?.id)}>
            <Trash2 size={15} />
          </button>
        </div>
        <div class="run-main cli-main">
          <div class="cli-output">
            <Show when={current()} keyed fallback={<Welcome />}>
              {(r) => <RunView run={r} scrollToEnd={scrollToEnd()} softWrap={softWrap()} />}
            </Show>
          </div>
          <ConsoleInput
            dropUp
            ref={(el) => (input = el)}
            running={isLive(current())}
            onStop={() => void stopRun(current()?.id)}
            onRun={(text) => void runInput(text, "toolWindow").then(() => input?.focus())}
          />
        </div>
      </div>
    </div>
  );
}
