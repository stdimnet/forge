/**
 * Run tool window (Alt+4): a tab per run with a vertical toolbar (Rerun, Stop, scroll to
 * end, soft wraps, clear, pin), the console and, for test runs, the test tree.
 */
import { createMemo, createSignal, For, Show, type JSX } from "solid-js";
import { createStore } from "solid-js/store";
import { Dynamic } from "solid-js/web";
import {
  ArrowDownAZ,
  ArrowDownToLine,
  ArrowDown,
  ArrowUp,
  Ban,
  ChevronsDownUp,
  ChevronsUpDown,
  CircleCheck,
  Clock,
  Pin,
  Play,
  RotateCcw,
  Square,
  Trash2,
  WrapText,
  X,
  Bug,
  RefreshCcwDot,
  ScrollText,
  History,
  TimerReset,
  CircleX,
} from "lucide-solid";
import { balloonsFor, shortcutLabel } from "@/keymap";
import { testFrameworksApi } from "@/ipc/testFrameworks";
import { getSetting } from "@/state/settings";
import { openMenu } from "./PopupMenu";
import { Console } from "./Console";
import { lineText, type TestNode } from "./tests";
import { findType } from "./types";
import { navigateToTest, StatusIcon, TestTree, type TreeView } from "./TestTree";
import {
  activeSession,
  activeSessionId,
  clearConsole,
  closeSession,
  isRunning,
  openHistoryRun,
  rerunFailedTests,
  toggleAutoTest,
  rerunSession,
  sendInput,
  sessionData,
  sessions,
  setActiveSession,
  stopSession,
  togglePin,
  type RunSession,
} from "./runStore";
import { openRunConfigurations } from "./dialogState";
import { saveTemporary, isTemporary } from "./configStore";
import { setDiffRequest } from "@/features/git/overlays";

interface SessionUi {
  scrollToEnd: boolean;
  softWrap?: boolean;
  selected: number | null;
  treeWidth: number;
  view: TreeView;
}

const [ui, setUi] = createStore<Record<string, SessionUi>>({});

function uiOf(id: string): SessionUi {
  if (!ui[id])
    setUi(id, {
      scrollToEnd: true,
      selected: null,
      treeWidth: 420,
      view: { showPassed: true, showIgnored: true, sort: "none", expanded: new Set(), collapsed: new Set() },
    });
  return ui[id];
}

/** Select a test node from outside (scenarios, "select first failed"). */
export function selectTestNode(sessionId: string, nodeId: number): void {
  uiOf(sessionId);
  setUi(sessionId, "selected", nodeId);
  setUi(sessionId, "scrollToEnd", false);
}

function ToolButton(props: {
  title: string;
  icon: JSX.Element;
  onClick: () => void;
  disabled?: boolean;
  active?: boolean;
  class?: string;
}) {
  return (
    <button
      class={`icon-btn run-tool ${props.class ?? ""}`}
      classList={{ active: !!props.active }}
      title={props.title}
      disabled={props.disabled}
      onClick={() => props.onClick()}
    >
      {props.icon}
    </button>
  );
}

const withKey = (title: string, action: string) => {
  const k = shortcutLabel(action);
  return k ? `${title} (${k})` : title;
};

function TabIcon(props: { session: RunSession }) {
  const type = () => findType(props.session.config.type);
  return (
    <span class="run-tab-icon" style={{ color: type()?.color }}>
      <Dynamic component={props.session.debug ? Bug : props.session.staticOutput ? ScrollText : (type()?.icon ?? Play)} size={14} />
      <Show when={isRunning(props.session)}>
        <span class="run-tab-live" />
      </Show>
    </span>
  );
}

/** Tabs in the tool window header. */
export function RunTabs() {
  return (
    <div class="run-tabs" role="tablist">
      <For each={sessions()}>
        {(s) => (
          <div
            class="run-tab"
            classList={{ active: activeSessionId() === s.id }}
            role="tab"
            data-run-tab={s.name}
            onClick={() => setActiveSession(s.id)}
            onMouseDown={(e) => e.button === 1 && (e.preventDefault(), void closeSession(s.id))}
            onContextMenu={(e) => {
              e.preventDefault();
              openMenu({
                x: e.clientX,
                y: e.clientY,
                items: [
                  { label: s.pinned ? "Unpin Tab" : "Pin Tab", icon: Pin, run: () => togglePin(s.id) },
                  { label: "Rerun", icon: RotateCcw, run: () => void rerunSession(s.id) },
                  { label: "Stop", icon: Square, disabled: !isRunning(s), run: () => void stopSession(s.id) },
                  { separator: true },
                  { label: "Edit Configuration…", run: () => openRunConfigurations(s.name) },
                  { label: "Save Configuration", disabled: !isTemporary(s.config), run: () => void saveTemporary(s.name) },
                  { separator: true },
                  { label: "Close Tab", icon: X, run: () => void closeSession(s.id) },
                  {
                    label: "Close Other Tabs",
                    disabled: sessions().length < 2,
                    run: () => sessions().filter((x) => x.id !== s.id && !x.pinned).forEach((x) => void closeSession(x.id)),
                  },
                ],
              });
            }}
          >
            <TabIcon session={s} />
            <span class="run-tab-title">{s.name}</span>
            <Show when={s.pinned}>
              <Pin size={11} class="muted" />
            </Show>
            <button
              class="run-tab-close"
              title="Close"
              onClick={(e) => {
                e.stopPropagation();
                void closeSession(s.id);
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

function showComparison(n: TestNode) {
  const path = n.location?.workspacePath || "Comparison Failure";
  setDiffRequest({
    path,
    left: { title: "Expected", content: n.expected ?? "" },
    right: { title: "Actual", content: n.actual ?? "" },
    editable: false,
  });
}

const historyBalloon = balloonsFor("Test History");

/** "Oct 23, 14:02". */
const when = (at: number) =>
  new Date(at).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

/** Test History: the last test runs of the project; picking one opens it in a Run tab. */
export async function showTestHistory(x: number, y: number): Promise<void> {
  const runs = await testFrameworksApi.history().catch((err) => {
    historyBalloon("Cannot read the test history", String(err));
    return null;
  });
  if (!runs) return;
  openMenu({
    x,
    y,
    items: runs.length
      ? runs.map((r) => ({
          label: `${r.name} – ${r.summary.replace(/ – .*$/, "")}`,
          icon: r.counts.failed ? CircleX : CircleCheck,
          iconColor: r.counts.failed ? "var(--danger)" : "var(--success)",
          shortcut: when(r.at),
          run: () =>
            void testFrameworksApi
              .openHistory(r.id)
              .then(openHistoryRun)
              .catch((err) => historyBalloon("Cannot open the test run", String(err))),
        }))
      : [{ label: "No test runs yet", disabled: true }],
  });
}

function findNode(root: TestNode, id: number | null): TestNode | undefined {
  if (id === null) return undefined;
  if (root.id === id) return root;
  for (const c of root.children) {
    const f = findNode(c, id);
    if (f) return f;
  }
  return undefined;
}

function failedNodes(root: TestNode): TestNode[] {
  const out: TestNode[] = [];
  const walk = (n: TestNode) => {
    if (n.kind === "test" && n.status === "failed") out.push(n);
    n.children.forEach(walk);
  };
  walk(root);
  return out;
}

function SessionView(props: { session: RunSession }) {
  const id = () => props.session.id;
  const u = () => uiOf(id());
  const d = () => sessionData(id());
  const tests = () => d()?.tests;
  const wrap = () => u().softWrap ?? getSetting<boolean>("run.console.softWrap", false);
  const running = () => isRunning(props.session);
  const selectedNode = createMemo(() => {
    props.session.version;
    const t = tests();
    return t ? findNode(t.root, u().selected) : undefined;
  });
  const consoleLines = () => {
    const n = selectedNode();
    if (n && n.kind === "test") return n.console;
    return d()?.lines ?? [];
  };
  const stepFailed = (delta: number) => {
    const t = tests();
    if (!t) return;
    const failed = failedNodes(t.root);
    if (!failed.length) return;
    const idx = failed.findIndex((f) => f.id === u().selected);
    const next = failed[(idx + delta + failed.length) % failed.length];
    selectTestNode(id(), next.id);
    void navigateToTest(next);
  };
  const [dragging, setDragging] = createSignal(false);
  const startDrag = (e: MouseEvent) => {
    const startX = e.clientX;
    const startW = u().treeWidth;
    setDragging(true);
    const move = (ev: MouseEvent) => setUi(id(), "treeWidth", Math.max(220, Math.min(900, startW + ev.clientX - startX)));
    const up = () => {
      setDragging(false);
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };
  const footer = () => {
    const n = selectedNode();
    if (!n || n.kind !== "test" || n.status !== "failed" || (n.expected === undefined && n.actual === undefined)) return undefined;
    return (
      <div class="run-line">
        <a class="run-link" onClick={() => showComparison(n)}>
          {"<Click to see difference>"}
        </a>
      </div>
    );
  };
  return (
    <div class="run-session" classList={{ dragging: dragging() }}>
      <div class="run-toolbar" role="toolbar" aria-orientation="vertical">
        <ToolButton
          title={withKey("Rerun", "Rerun")}
          class="run-rerun"
          icon={<RotateCcw size={15} />}
          onClick={() => void rerunSession(id())}
        />
        <ToolButton
          title={withKey("Stop", "Stop")}
          class="run-stop"
          icon={<Square size={13} fill="currentColor" />}
          disabled={!running()}
          onClick={() => void stopSession(id())}
        />
        <Show when={tests()}>
          <ToolButton
            title="Rerun Failed Tests"
            icon={<RefreshCcwDot size={15} />}
            disabled={running() || !tests() || tests()!.summary.counts.failed === 0}
            onClick={() => void rerunFailedTests(id())}
          />
          <ToolButton
            title="Toggle Auto-Test"
            class="run-auto-test"
            icon={<TimerReset size={15} />}
            active={!!props.session.autoTest}
            onClick={() => toggleAutoTest(id())}
          />
        </Show>
        <div class="run-tool-sep" />
        <ToolButton
          title="Scroll to the End"
          icon={<ArrowDownToLine size={15} />}
          active={u().scrollToEnd}
          onClick={() => setUi(id(), "scrollToEnd", (v) => !v)}
        />
        <ToolButton title="Soft-Wrap" icon={<WrapText size={15} />} active={wrap()} onClick={() => setUi(id(), "softWrap", !wrap())} />
        <ToolButton title="Clear All" icon={<Trash2 size={15} />} onClick={() => clearConsole(id())} />
        <div class="run-tool-sep" />
        <ToolButton
          title={props.session.pinned ? "Unpin Tab" : "Pin Tab"}
          icon={<Pin size={15} />}
          active={props.session.pinned}
          onClick={() => togglePin(id())}
        />
      </div>
      <div class="run-main">
        <Show
          when={tests()}
          fallback={
            <Console
              lines={consoleLines}
              version={() => props.session.version}
              softWrap={wrap()}
              scrollToEnd={u().scrollToEnd}
              onInput={running() ? (t) => void sendInput(id(), t) : undefined}
            />
          }
        >
          {(t) => {
            const status = createMemo(() => {
              props.session.version;
              return t().summary;
            });
            const view = () => u().view;
            const toggleView = (k: "showPassed" | "showIgnored") => setUi(id(), "view", k, (v) => !v);
            return (
              <div class="run-tests">
                <div class="run-tests-bar">
                  <div class="run-tests-tools" style={{ width: `${u().treeWidth}px` }}>
                    <ToolButton title="Show Passed" icon={<CircleCheck size={15} />} active={view().showPassed} onClick={() => toggleView("showPassed")} />
                    <ToolButton title="Show Ignored" icon={<Ban size={15} />} active={view().showIgnored} onClick={() => toggleView("showIgnored")} />
                    <div class="run-tool-sep vertical" />
                    <ToolButton
                      title="Sort Alphabetically"
                      icon={<ArrowDownAZ size={15} />}
                      active={view().sort === "alpha"}
                      onClick={() => setUi(id(), "view", "sort", (s) => (s === "alpha" ? "none" : "alpha"))}
                    />
                    <ToolButton
                      title="Sort by Duration"
                      icon={<Clock size={15} />}
                      active={view().sort === "duration"}
                      onClick={() => setUi(id(), "view", "sort", (s) => (s === "duration" ? "none" : "duration"))}
                    />
                    <div class="run-tool-sep vertical" />
                    <ToolButton title="Expand All" icon={<ChevronsUpDown size={15} />} onClick={() => setUi(id(), "view", "collapsed", new Set())} />
                    <ToolButton
                      title="Collapse All"
                      icon={<ChevronsDownUp size={15} />}
                      onClick={() => {
                        const ids = new Set<number>();
                        const walk = (n: TestNode) => {
                          if (n.kind === "suite") ids.add(n.id);
                          n.children.forEach(walk);
                        };
                        walk(t().root);
                        setUi(id(), "view", "collapsed", ids);
                      }}
                    />
                    <ToolButton title="Previous Failed Test" icon={<ArrowUp size={15} />} onClick={() => stepFailed(-1)} />
                    <ToolButton title="Next Failed Test" icon={<ArrowDown size={15} />} onClick={() => stepFailed(1)} />
                    <div class="run-tool-sep vertical" />
                    <button
                      class="icon-btn run-tool run-test-history"
                      title="Test History"
                      onClick={(e) => {
                        const r = e.currentTarget.getBoundingClientRect();
                        void showTestHistory(r.left, r.bottom + 2);
                      }}
                    >
                      <History size={15} />
                    </button>
                  </div>
                  <div class="run-tests-status" classList={{ failed: status().failed }}>
                    <StatusIcon status={running() ? "running" : t().root.status} />
                    <span>{status().text}</span>
                  </div>
                  <div
                    class="run-progress"
                    classList={{ failed: status().failed }}
                    style={{ width: `${status().total ? (100 * status().done) / status().total : running() ? 0 : 100}%` }}
                  />
                </div>
                <div class="run-tests-body">
                  <div class="run-tests-tree" style={{ width: `${u().treeWidth}px` }}>
                    <TestTree
                      root={t().root}
                      version={props.session.version}
                      view={view()}
                      selected={u().selected ?? t().root.id}
                      onSelect={(n) => selectTestNode(id(), n.id)}
                      onToggle={(n) =>
                        setUi(id(), "view", "collapsed", (c) => {
                          const next = new Set(c);
                          if (next.has(n.id)) next.delete(n.id);
                          else next.add(n.id);
                          return next;
                        })
                      }
                    />
                  </div>
                  <div class="run-splitter" onMouseDown={startDrag} />
                  <Console
                    lines={consoleLines}
                    version={() => props.session.version + (u().selected ?? -1) * 1e6}
                    softWrap={wrap()}
                    scrollToEnd={u().scrollToEnd && (!selectedNode() || selectedNode()!.kind === "root")}
                    onInput={running() ? (text) => void sendInput(id(), text) : undefined}
                    footer={footer()}
                  />
                </div>
              </div>
            );
          }}
        </Show>
      </div>
    </div>
  );
}

export function RunToolWindow() {
  return (
    <div class="run-window">
      <Show
        when={activeSession()}
        keyed
        fallback={
          <div class="run-empty-window muted">
            <p>Nothing to show.</p>
            <p>
              Run a configuration with <b>{shortcutLabel("Run") || "Shift+F10"}</b> or open{" "}
              <button class="link-btn" onClick={() => openRunConfigurations()}>
                Run/Debug Configurations
              </button>
              .
            </p>
          </div>
        }
      >
        {(s) => <SessionView session={s} />}
      </Show>
    </div>
  );
}

/** Plain text of the active console (for tests and "Copy"). */
export const activeConsoleText = () => (activeSession() ? (sessionData(activeSession()!.id)?.lines ?? []).map(lineText).join("\n") : "");
