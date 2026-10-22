/** Debug tool window (Alt+5): toolbar, Frames, Variables with Watches, Console. */
import { createEffect, createSignal, For, on, Show, type Component } from "solid-js";
import {
  ArrowDownToLine,
  ArrowRightToLine,
  ArrowUpFromLine,
  Bug,
  Calculator,
  CircleDot,
  CircleSlash,
  Locate,
  Pause,
  PhoneCall,
  PhoneOff,
  Play,
  RedoDot,
  Square,
  Trash2,
  X,
} from "lucide-solid";
import { runAction, shortcutLabel } from "@/keymap";
import { formatForKeymap } from "@/keymap/store";
import { debugApi } from "./api";
import { basename, frameLabel } from "./format";
import {
  activeId,
  activeSession,
  addWatch,
  breakpointsMuted,
  clearConsole,
  consoleLines,
  evaluateInConsole,
  frameIndex,
  frames,
  listening,
  pausedSession,
  port,
  removeWatch,
  selectFrame,
  selectSession,
  sessions,
  toggleListening,
  variables,
  variablesLoading,
  watches,
  watchResults,
} from "./store";
import { VariableNode } from "./VariablesTree";
import { setEvaluateRequest } from "./overlays";
import { ExpressionInput } from "./ExpressionInput";

const tip = (title: string, action: string) => {
  const keys = shortcutLabel(action);
  return keys ? `${title} (${formatForKeymap(keys)})` : title;
};

function ToolButton(props: {
  action: string;
  title: string;
  icon: Component<{ size?: number; class?: string }>;
  enabled: boolean;
  tone?: "run" | "stop" | "bp" | "force";
  pressed?: boolean;
}) {
  return (
    <button
      class="icon-btn dbg-tool"
      classList={{ [`dbg-tone-${props.tone}`]: !!props.tone, pressed: !!props.pressed }}
      title={tip(props.title, props.action)}
      disabled={!props.enabled}
      data-action={props.action}
      onClick={() => void runAction(props.action)}
    >
      <props.icon size={15} />
    </button>
  );
}

function sessionTitle(id: string): string {
  const s = sessions().find((x) => x.id === id);
  if (!s) return id;
  const file = s.file ?? s.fileUri.replace(/^file:\/\//, "");
  return s.serverName === "CLI" ? basename(file) : s.serverName;
}

export function DebugActions() {
  return (
    <button
      class="icon-btn"
      classList={{ "dbg-listening": listening() }}
      title={listening() ? "Stop Listening for PHP Debug Connections" : "Start Listening for PHP Debug Connections"}
      onClick={() => void toggleListening()}
    >
      {listening() ? <PhoneCall size={15} /> : <PhoneOff size={15} />}
    </button>
  );
}

function Toolbar(props: { tab: string; setTab: (t: string) => void }) {
  const paused = () => !!pausedSession();
  const running = () => activeSession()?.status === "running";
  const alive = () => !!activeSession();
  return (
    <div class="dbg-toolbar">
      <div class="dbg-tabs" role="tablist">
        <button class="dbg-tab" classList={{ active: props.tab === "frames" }} onClick={() => props.setTab("frames")}>
          Threads &amp; Variables
        </button>
        <button class="dbg-tab" classList={{ active: props.tab === "console" }} onClick={() => props.setTab("console")}>
          Console
        </button>
      </div>
      <div class="dbg-sep" />
      <ToolButton action="Resume" title="Resume Program" icon={Play} enabled={paused()} tone="run" />
      <ToolButton action="Pause" title="Pause Program" icon={Pause} enabled={running()} />
      <ToolButton action="Stop" title="Stop" icon={Square} enabled={alive()} tone="stop" />
      <div class="dbg-sep" />
      <ToolButton action="ShowExecutionPoint" title="Show Execution Point" icon={Locate} enabled={paused()} />
      <ToolButton action="StepOver" title="Step Over" icon={RedoDot} enabled={paused()} />
      <ToolButton action="StepInto" title="Step Into" icon={ArrowDownToLine} enabled={paused()} />
      <ToolButton action="ForceStepInto" title="Force Step Into" icon={ArrowDownToLine} enabled={paused()} tone="force" />
      <ToolButton action="StepOut" title="Step Out" icon={ArrowUpFromLine} enabled={paused()} />
      <ToolButton action="RunToCursor" title="Run to Cursor" icon={ArrowRightToLine} enabled={paused()} />
      <ToolButton action="EvaluateExpression" title="Evaluate Expression…" icon={Calculator} enabled={paused()} />
      <div class="dbg-sep" />
      <ToolButton action="ViewBreakpoints" title="View Breakpoints…" icon={CircleDot} enabled tone="bp" />
      <ToolButton action="XDebugger.MuteBreakpoints" title="Mute Breakpoints" icon={CircleSlash} enabled pressed={breakpointsMuted()} />
    </div>
  );
}

function SessionTabs() {
  return (
    <Show when={sessions().length > 0}>
      <div class="dbg-sessions" role="tablist">
        <For each={sessions()}>
          {(s) => (
            <div
              class="dbg-session-tab"
              classList={{ active: s.id === activeId(), paused: s.status === "paused" }}
              onClick={() => selectSession(s.id)}
              title={`${s.serverName} • ${s.fileUri}`}
            >
              <Bug size={13} />
              <span>{sessionTitle(s.id)}</span>
              <button
                class="dbg-session-close"
                title="Stop and close"
                onClick={(e) => {
                  e.stopPropagation();
                  void debugApi.stop(s.id).catch(() => {});
                }}
              >
                <X size={11} />
              </button>
            </div>
          )}
        </For>
      </div>
    </Show>
  );
}

function FramesPane() {
  return (
    <div class="dbg-frames" role="listbox" aria-label="Frames">
      <Show
        when={pausedSession()}
        fallback={<div class="dbg-empty muted">{activeSession()?.status === "running" ? "The application is running" : "Frames are not available"}</div>}
      >
        <For each={frames()}>
          {(f, i) => {
            const label = frameLabel(f);
            return (
              <div
                class="tree-row dbg-frame"
                classList={{ selected: i() === frameIndex(), external: !f.file }}
                onClick={() => void selectFrame(i())}
                data-frame={i()}
                title={f.fileUri}
              >
                <span class="dbg-frame-loc">{label.location},</span>
                <span class="dbg-frame-where">{label.where}</span>
              </div>
            );
          }}
        </For>
      </Show>
    </div>
  );
}

function VariablesPane() {
  const [expr, setExpr] = createSignal("");
  const submit = (asWatch: boolean) => {
    const e = expr().trim();
    if (!e) return;
    if (asWatch) addWatch(e);
    else setEvaluateRequest({ expression: e });
    setExpr("");
  };
  return (
    <div class="dbg-vars">
      <div class="dbg-eval-row">
        <input
          class="dbg-eval-input"
          placeholder={`Evaluate expression (Enter) or add a watch (${formatForKeymap("Ctrl+Shift+Enter")})`}
          value={expr()}
          disabled={!pausedSession()}
          onInput={(e) => setExpr(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key !== "Enter") return;
            e.preventDefault();
            submit(e.ctrlKey || e.metaKey);
          }}
        />
      </div>
      <div class="dbg-tree" role="tree" tabIndex={0} aria-label="Variables">
        <Show when={pausedSession()} fallback={<div class="dbg-empty muted">{activeSession() ? "Variables are not available" : ""}</div>}>
          <For each={watches()}>
            {(w) => {
              const r = () => watchResults()[w];
              const value = () => {
                const x = r();
                return x && "value" in x ? x.value : undefined;
              };
              return (
                <Show
                  when={value()}
                  fallback={
                    <VariableNode
                      p={{ name: w, fullname: w, type: "undefined", hasChildren: false, numChildren: 0, page: 0, pageSize: 0, children: [] }}
                      label={w}
                      depth={0}
                      watch
                      onRemove={() => removeWatch(w)}
                      selectable
                    >
                      <span class="dbg-eq"> = </span>
                      <span class="dbg-error-text">{(() => { const x = r(); return x && "error" in x ? x.error : "Collecting data…"; })()}</span>
                    </VariableNode>
                  }
                >
                  {(p) => <VariableNode p={p()} label={w} depth={0} watch onRemove={() => removeWatch(w)} selectable />}
                </Show>
              );
            }}
          </For>
          <For each={variables()} fallback={<Show when={!variablesLoading()}><div class="dbg-empty muted">No variables</div></Show>}>
            {(v) => <VariableNode p={v} depth={0} selectable />}
          </For>
        </Show>
      </div>
    </div>
  );
}

function ConsolePane() {
  let el!: HTMLDivElement;
  const [expr, setExpr] = createSignal("");
  const lines = () => consoleLines().filter((l) => l.sessionId === activeId());
  createEffect(on(lines, () => queueMicrotask(() => el && (el.scrollTop = el.scrollHeight))));
  return (
    <div class="dbg-console-wrap">
      <div class="dbg-console-tools">
        <button class="icon-btn" title="Clear All" onClick={clearConsole}>
          <Trash2 size={14} />
        </button>
      </div>
      <div class="dbg-console-main">
        <div class="dbg-console" ref={el}>
          <For each={lines()} fallback={<span class="muted">No output yet.</span>}>
            {(l) => (
              <Show when={l.value} fallback={<span class={`dbg-out dbg-out-${l.stream}`}>{l.text}</span>}>
                {(v) => (
                  <span class="dbg-console-value dbg-tree">
                    <VariableNode p={v()} depth={0} initiallyOpen={false} />
                  </span>
                )}
              </Show>
            )}
          </For>
        </div>
        <div class="dbg-console-input">
          <span class="dbg-console-prompt">&gt;</span>
          <ExpressionInput
            value={expr()}
            onInput={setExpr}
            onSubmit={(e) => {
              if (!e.trim()) return;
              setExpr("");
              void evaluateInConsole(e);
            }}
            disabled={!activeSession()}
            placeholder={pausedSession() ? "Evaluate a PHP expression in the current frame" : "Expressions can be evaluated while the script is suspended"}
          />
        </div>
      </div>
    </div>
  );
}

function EmptyState() {
  return (
    <div class="dbg-welcome">
      <Show
        when={listening()}
        fallback={
          <>
            <PhoneOff size={28} class="muted" />
            <p>Forge is not listening for PHP debug connections.</p>
            <button class="btn-primary" onClick={() => void toggleListening()}>
              Start Listening
            </button>
            <p class="muted dbg-hint">Xdebug 3 connects to port {port()} (xdebug.mode=debug, xdebug.start_with_request=yes).</p>
          </>
        }
      >
        <PhoneCall size={28} class="dbg-listening" />
        <p>Waiting for incoming connection on port {port()}…</p>
        <p class="muted dbg-hint">Start a request or a CLI script with Xdebug enabled. Breakpoints suspend execution here.</p>
      </Show>
    </div>
  );
}

export function DebugView() {
  const [tab, setTab] = createSignal("frames");
  return (
    <div class="dbg-view">
      <Show when={sessions().length > 0} fallback={<EmptyState />}>
        <SessionTabs />
        <Toolbar tab={tab()} setTab={setTab} />
        <Show when={tab() === "frames"} fallback={<ConsolePane />}>
          <div class="dbg-body">
            <FramesPane />
            <VariablesPane />
          </div>
        </Show>
      </Show>
    </div>
  );
}
