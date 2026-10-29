/**
 * The Python Console tool window (PyCharm's Python Console): the REPL's output, a prompt
 * with history (Up / Down) and multi-line input, and the Variables view of the console's
 * namespace with expandable values.
 */
import { createSignal, For, onMount, Show } from "solid-js";
import { ChevronDown, ChevronRight, Eraser, Play, RotateCw, Square, Variable } from "lucide-solid";
import { Console } from "@/features/run/Console";
import type { ConsoleVariable } from "@/ipc/pythonTools";
import {
  clearConsole,
  consoleHistory,
  consoleLines,
  consoleState,
  consoleVersion,
  restartConsole,
  startConsole,
  stopConsole,
  submitLine,
  toggleExpanded,
  toggleVariables,
} from "./store";

export function ConsoleActions() {
  const running = () => consoleState.status === "running" || consoleState.status === "starting";
  return (
    <>
      <button class="icon-btn" title="Rerun Python Console" onClick={() => void restartConsole()}>
        <RotateCw size={14} />
      </button>
      <button class="icon-btn" title="Stop Console" disabled={!running()} onClick={() => void stopConsole()}>
        <Square size={13} />
      </button>
      <button
        class="icon-btn"
        classList={{ active: consoleState.showVariables }}
        title="Show Variables"
        aria-pressed={consoleState.showVariables}
        onClick={toggleVariables}
      >
        <Variable size={14} />
      </button>
      <button class="icon-btn" title="Clear All" onClick={clearConsole}>
        <Eraser size={14} />
      </button>
    </>
  );
}

function VariableRows(props: { vars: ConsoleVariable[]; depth: number }) {
  return (
    <For each={props.vars}>
      {(v) => (
        <>
          <div class="pycon-var tree-row" style={{ "padding-left": `${6 + props.depth * 14}px` }} title={`${v.name} = ${v.value}`} data-expr={v.expr}>
            <span class="pycon-var-toggle" onClick={() => v.children && toggleExpanded(v)}>
              <Show when={v.children}>{consoleState.expanded[v.expr] ? <ChevronDown size={12} /> : <ChevronRight size={12} />}</Show>
            </span>
            <span class="pycon-var-name">{v.name}</span>
            <span class="pycon-var-eq"> = </span>
            <span class="pycon-var-type muted">
              {"{"}
              {v.type}
              {"}"}
              {v.size != null ? ` ${v.size}` : ""}
            </span>
            <span class="pycon-var-value">{v.value}</span>
          </div>
          <Show when={consoleState.expanded[v.expr] && consoleState.children[v.expr]}>
            <VariableRows vars={consoleState.children[v.expr]} depth={props.depth + 1} />
          </Show>
        </>
      )}
    </For>
  );
}

function PromptInput() {
  const [text, setText] = createSignal("");
  let at = -1;
  let draft = "";
  let el!: HTMLTextAreaElement;
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey) {
      e.preventDefault();
      const value = text();
      setText("");
      at = -1;
      void submitLine(value);
      return;
    }
    const history = consoleHistory();
    if ((e.key === "ArrowUp" || e.key === "ArrowDown") && !text().includes("\n") && history.length) {
      e.preventDefault();
      if (at === -1) draft = text();
      if (e.key === "ArrowUp") at = at === -1 ? history.length - 1 : Math.max(0, at - 1);
      else at = at === -1 ? -1 : at + 1;
      if (at >= history.length) at = -1;
      setText(at === -1 ? draft : history[at]);
    }
  };
  onMount(() => el.focus());
  return (
    <div class="pycon-prompt" onClick={() => el.focus()}>
      <span class="pycon-prompt-label">{consoleState.prompt}</span>
      <textarea
        ref={el}
        class="pycon-input"
        rows={Math.min(8, text().split("\n").length)}
        spellcheck={false}
        value={text()}
        disabled={consoleState.status !== "running"}
        placeholder={consoleState.status === "running" ? "" : consoleState.status === "starting" ? "Starting the console…" : "The console is not running"}
        onInput={(e) => setText(e.currentTarget.value)}
        onKeyDown={onKeyDown}
        data-testid="python-console-input"
      />
    </div>
  );
}

export function ConsoleView() {
  onMount(() => {
    if (consoleState.status === "idle") void startConsole();
  });
  return (
    <div class="pycon" data-testid="python-console">
      <div class="pycon-main">
        <div class="pycon-output">
          <Console lines={consoleLines} version={consoleVersion} softWrap={true} scrollToEnd={true} emptyText="Starting the Python Console…" />
        </div>
        <PromptInput />
      </div>
      <Show when={consoleState.showVariables}>
        <div class="pycon-vars" data-testid="python-console-variables">
          <div class="pycon-vars-head">
            <Variable size={12} /> Variables
            <span class="muted">{consoleState.interpreter}</span>
          </div>
          <div class="pycon-vars-list">
            <Show when={consoleState.vars.length} fallback={<div class="muted pycon-vars-empty">No variables yet</div>}>
              <VariableRows vars={consoleState.vars} depth={0} />
            </Show>
          </div>
        </div>
      </Show>
      <Show when={consoleState.status === "exited"}>
        <button class="pill-btn pycon-restart" onClick={() => void restartConsole()}>
          <Play size={12} /> Start Console
        </button>
      </Show>
    </div>
  );
}
