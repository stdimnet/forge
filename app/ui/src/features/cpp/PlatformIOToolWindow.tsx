/**
 * The PlatformIO tool window, like CLion's: the environments of `platformio.ini` (default
 * ones first and bold) with their platform, board and frameworks, and the project tasks of
 * each (Build, Upload, Monitor, Test, Clean, Compilation Database). Double-click a task to
 * run it (`pio run -e <env> -t <target>` in the Run tool window); double-click an
 * environment to open its section.
 */
import { createSignal, For, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { ChevronDown, ChevronRight, CircuitBoard, Database, FlaskConical, Hammer, MonitorDot, Play, RefreshCw, Trash2, TriangleAlert, Upload } from "lucide-solid";
import type { PioEnv, PioTask } from "@/ipc/cppTools";
import { openFileAt } from "@/editor/views";
import { embeddedInfo, refreshEmbedded, runPioTask } from "./embedded";

const TASKS: { task: PioTask; label: string; icon: typeof Hammer }[] = [
  { task: "build", label: "Build", icon: Hammer },
  { task: "upload", label: "Upload", icon: Upload },
  { task: "monitor", label: "Monitor", icon: MonitorDot },
  { task: "test", label: "Test", icon: FlaskConical },
  { task: "clean", label: "Clean", icon: Trash2 },
  { task: "compiledb", label: "Compilation Database", icon: Database },
];

const [selected, setSelected] = createSignal<string | null>(null);
const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());

function toggle(key: string) {
  const next = new Set(collapsed());
  if (next.has(key)) next.delete(key);
  else next.add(key);
  setCollapsed(next);
}

/** The selected environment's name ("" = the default environments). */
const selectedEnv = () => (selected() ?? "").split("/")[0];

function TaskRows(props: { env: string; indent: number }) {
  return (
    <For each={TASKS}>
      {(t) => {
        const key = `${props.env}/${t.task}`;
        return (
          <div
            class="tree-row pio-row pio-task"
            classList={{ selected: selected() === key }}
            style={{ "padding-left": `${props.indent}px` }}
            data-pio-task={key}
            onClick={() => setSelected(key)}
            onDblClick={() => void runPioTask(props.env, t.task)}
            title={`Double-click to run: pio ${t.task === "monitor" ? "device monitor" : t.task === "test" ? "test" : "run"}${props.env ? ` -e ${props.env}` : ""}`}
          >
            <Dynamic component={t.icon} size={14} />
            <span>{t.label}</span>
          </div>
        );
      }}
    </For>
  );
}

function EnvNode(props: { env: PioEnv }) {
  const open = () => !collapsed().has(props.env.name);
  const details = () => [props.env.platform, props.env.board, props.env.framework.join(", ")].filter(Boolean).join(" · ");
  return (
    <>
      <div
        class="tree-row pio-row pio-env"
        classList={{ selected: selected() === props.env.name, default: props.env.default }}
        data-pio-env={props.env.name}
        onClick={() => {
          setSelected(props.env.name);
          toggle(props.env.name);
        }}
        onDblClick={() => void openFileAt("platformio.ini", props.env.line)}
      >
        {open() ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        <CircuitBoard size={14} class="pio-env-icon" />
        <span class="pio-env-name">{props.env.name}</span>
        <span class="muted pio-env-details">{details()}</span>
      </div>
      <Show when={open()}>
        <TaskRows env={props.env.name} indent={44} />
      </Show>
    </>
  );
}

export function PlatformIOToolWindow() {
  const project = () => embeddedInfo()?.platformio;
  const run = (task: PioTask) => void runPioTask(selectedEnv(), task);
  return (
    <div class="pio-window">
      <div class="pio-toolbar">
        <button class="icon-btn" title="Refresh PlatformIO Project" onClick={() => void refreshEmbedded()}>
          <RefreshCw size={14} />
        </button>
        <button class="icon-btn" title="Build the selected environment" onClick={() => run("build")}>
          <Hammer size={14} />
        </button>
        <button class="icon-btn" title="Upload the selected environment" onClick={() => run("upload")}>
          <Upload size={14} />
        </button>
        <button class="icon-btn" title="Run the selected task" disabled={!selected()?.includes("/")} onClick={() => run(selected()!.split("/")[1] as PioTask)}>
          <Play size={14} />
        </button>
      </div>
      <Show when={project()} fallback={<div class="muted pio-empty">No platformio.ini in the project.</div>}>
        {(p) => (
          <div class="pio-tree" role="tree">
            <For each={p().problems ?? []}>
              {(problem) => (
                <div class="tree-row pio-row pio-problem">
                  <TriangleAlert size={13} />
                  <span>{problem}</span>
                </div>
              )}
            </For>
            <div class="tree-row pio-row pio-group" onClick={() => toggle("#default")}>
              {collapsed().has("#default") ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
              <span>Default</span>
              <span class="muted">{p().defaultEnvs.length ? p().defaultEnvs.join(", ") : "all environments"}</span>
            </div>
            <Show when={!collapsed().has("#default")}>
              <TaskRows env="" indent={28} />
            </Show>
            <div class="tree-row pio-row pio-group" onClick={() => toggle("#envs")}>
              {collapsed().has("#envs") ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
              <span>Environments</span>
              <span class="muted">{p().envs.length}</span>
            </div>
            <Show when={!collapsed().has("#envs")}>
              <For each={p().envs}>{(env) => <EnvNode env={env} />}</For>
            </Show>
          </div>
        )}
      </Show>
    </div>
  );
}
