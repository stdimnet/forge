/**
 * Debug tool window (Alt+5): toolbar, Frames, Variables with Watches, Console, and for
 * debug adapter sessions the threads and the Memory, Disassembly and Registers views.
 */
import { createEffect, createSignal, For, Match, on, Show, Switch, type Component } from "solid-js";
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
  RotateCcw,
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
  sessionFeatures,
} from "./store";
import { debugTab, DisassemblyPane, MemoryPane, RegistersPane, setDebugTab, ThreadSelector, type DebugTab } from "./nativeViews";
import { VariableNode } from "./VariablesTree";
import { debugTabs } from "./debugTabs";
import { setEvaluateRequest } from "./overlays";
import { ExpressionInput } from "./ExpressionInput";
import { t as tr } from "@/i18n";

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
  if (s.adapter) return s.serverName;
  const file = s.file ?? s.fileUri.replace(/^file:\/\//, "");
  return s.serverName === "CLI" ? basename(file) : s.serverName;
}

export function DebugActions() {
  return (
    <button
      class="icon-btn"
      classList={{ "dbg-listening": listening() }}
      title={listening() ? tr("debug.debugView.stopListeningForPhpDebug") : tr("debug.debugView.startListeningForPhpDebug")}
      onClick={() => void toggleListening()}
    >
      {listening() ? <PhoneCall size={15} /> : <PhoneOff size={15} />}
    </button>
  );
}

function Tab(props: { id: DebugTab; title: string }) {
  return (
    <button class="dbg-tab" classList={{ active: debugTab() === props.id }} data-tab={props.id} onClick={() => setDebugTab(props.id)}>
      {props.title}
    </button>
  );
}

function Toolbar() {
  const paused = () => !!pausedSession();
  const running = () => activeSession()?.status === "running";
  const alive = () => !!activeSession();
  const f = sessionFeatures;
  return (
    <div class="dbg-toolbar">
      <div class="dbg-tabs" role="tablist">
        <Tab id="frames" title={tr("debug.debugView.threadsVariables")} />
        <Tab id="console" title={tr("debug.debugView.console")} />
        <Show when={f()?.readMemory}>
          <Tab id="memory" title={tr("debug.debugView.memory")} />
        </Show>
        <Show when={f()?.disassemble}>
          <Tab id="disassembly" title={tr("debug.debugView.disassembly")} />
        </Show>
        <Show when={f()?.registers}>
          <Tab id="registers" title={tr("debug.debugView.registers")} />
        </Show>
        <For each={debugTabs().filter((t) => t.when(activeSession()))}>{(t) => <Tab id={t.id} title={t.title} />}</For>
      </div>
      <div class="dbg-sep" />
      <Show when={f()?.restart}>
        <ToolButton action="Forge.Debugger.Rerun" title={tr("debug.debugView.rerun")} icon={RotateCcw} enabled={alive()} tone="run" />
      </Show>
      <ToolButton action="Resume" title={tr("debug.debugView.resumeProgram")} icon={Play} enabled={paused()} tone="run" />
      <ToolButton action="Pause" title={tr("debug.debugView.pauseProgram")} icon={Pause} enabled={running()} />
      <ToolButton action="Stop" title={tr("debug.debugView.stop")} icon={Square} enabled={alive()} tone="stop" />
      <div class="dbg-sep" />
      <ToolButton action="ShowExecutionPoint" title={tr("debug.debugView.showExecutionPoint")} icon={Locate} enabled={paused()} />
      <ToolButton action="StepOver" title={tr("debug.debugView.stepOver")} icon={RedoDot} enabled={paused()} />
      <ToolButton action="StepInto" title={tr("debug.debugView.stepInto")} icon={ArrowDownToLine} enabled={paused()} />
      <ToolButton action="ForceStepInto" title={tr("debug.debugView.forceStepInto")} icon={ArrowDownToLine} enabled={paused()} tone="force" />
      <ToolButton action="StepOut" title={tr("debug.debugView.stepOut")} icon={ArrowUpFromLine} enabled={paused()} />
      <ToolButton action="RunToCursor" title={tr("debug.debugView.runToCursor")} icon={ArrowRightToLine} enabled={paused()} />
      <ToolButton action="EvaluateExpression" title={tr("debug.debugView.evaluateExpression")} icon={Calculator} enabled={paused()} />
      <div class="dbg-sep" />
      <ToolButton action="ViewBreakpoints" title={tr("debug.debugView.viewBreakpoints")} icon={CircleDot} enabled tone="bp" />
      <ToolButton action="XDebugger.MuteBreakpoints" title={tr("debug.debugView.muteBreakpoints")} icon={CircleSlash} enabled pressed={breakpointsMuted()} />
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
                title={tr("debug.debugView.stopAndClose")}
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
    <div class="dbg-frames" role="listbox" aria-label={tr("debug.debugView.frames")}>
      <ThreadSelector />
      <Show
        when={pausedSession()}
        fallback={<div class="dbg-empty muted">{activeSession()?.status === "running" ? tr("debug.debugView.theApplicationIsRunning") : tr("debug.debugView.framesAreNotAvailable")}</div>}
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
      <div class="dbg-tree" role="tree" tabIndex={0} aria-label={tr("debug.debugView.variables")}>
        <Show when={pausedSession()} fallback={<div class="dbg-empty muted">{activeSession() ? tr("debug.debugView.variablesAreNotAvailable") : ""}</div>}>
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
                      <span class="dbg-error-text">{(() => { const x = r(); return x && "error" in x ? x.error : tr("debug.debugView.collectingData"); })()}</span>
                    </VariableNode>
                  }
                >
                  {(p) => <VariableNode p={p()} label={w} depth={0} watch onRemove={() => removeWatch(w)} selectable />}
                </Show>
              );
            }}
          </For>
          <For each={variables()} fallback={<Show when={!variablesLoading()}><div class="dbg-empty muted">{tr("debug.debugView.noVariables")}</div></Show>}>
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
        <button class="icon-btn" title={tr("debug.debugView.clearAll")} onClick={clearConsole}>
          <Trash2 size={14} />
        </button>
      </div>
      <div class="dbg-console-main">
        <div class="dbg-console" ref={el}>
          <For each={lines()} fallback={<span class="muted">{tr("debug.debugView.noOutputYet")}</span>}>
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
            placeholder={
              pausedSession()
                ? `Evaluate ${activeSession()?.adapter ? "an expression or debugger command" : "a PHP expression"} in the current frame`
                : "Expressions can be evaluated while the program is suspended"
            }
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
            <p>{tr("debug.debugView.forgeIsNotListeningFor")}</p>
            <button class="btn-primary" onClick={() => void toggleListening()}>
              {tr("debug.debugView.startListening")}
            </button>
            <p class="muted dbg-hint">{tr("debug.debugView.xdebugConnectsToPort", { port: port() })}</p>
            <p class="muted dbg-hint">
              {tr("debug.debugView.otherLanguages")}{" "}
              <button class="dbg-link" onClick={() => void runAction("XDebugger.AttachToProcess")}>
                {tr("debug.debugView.attachToProcess")}
              </button>
            </p>
          </>
        }
      >
        <PhoneCall size={28} class="dbg-listening" />
        <p>{tr("debug.debugView.waitingOnPort", { port: port() })}</p>
        <p class="muted dbg-hint">{tr("debug.debugView.startARequestOrA")}</p>
      </Show>
    </div>
  );
}

export function DebugView() {
  // A tab the new session does not offer falls back to the frames.
  createEffect(() => {
    const t = debugTab();
    const f = sessionFeatures();
    if ((t === "memory" && !f?.readMemory) || (t === "disassembly" && !f?.disassemble) || (t === "registers" && !f?.registers)) setDebugTab("frames");
    const extra = debugTabs().find((x) => x.id === t);
    if (extra && !extra.when(activeSession())) setDebugTab("frames");
  });
  return (
    <div class="dbg-view">
      <Show when={sessions().length > 0} fallback={<EmptyState />}>
        <SessionTabs />
        <Toolbar />
        <Switch>
          <Match when={debugTab() === "console"}>
            <ConsolePane />
          </Match>
          <Match when={debugTab() === "memory"}>
            <MemoryPane />
          </Match>
          <Match when={debugTab() === "disassembly"}>
            <div class="dbg-body">
              <FramesPane />
              <DisassemblyPane />
            </div>
          </Match>
          <Match when={debugTab() === "registers"}>
            <div class="dbg-body">
              <FramesPane />
              <RegistersPane />
            </div>
          </Match>
          <Match when={debugTabs().find((t) => t.id === debugTab())}>
            {(t) => {
              const View = t().component;
              return <View />;
            }}
          </Match>
          <Match when={true}>
            <div class="dbg-body">
              <FramesPane />
              <VariablesPane />
            </div>
          </Match>
        </Switch>
      </Show>
    </div>
  );
}
