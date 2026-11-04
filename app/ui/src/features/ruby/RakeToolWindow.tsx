/**
 * The Rake tool window, like RubyMine's: the project's tasks grouped by namespace
 * (`db:migrate` under `db`), with their arguments and descriptions, from `rake --tasks
 * --all` in trusted projects or read from the Rakefile and `.rake` files otherwise
 * (`ruby_rake_tasks`, parsed in Rust).
 *
 * Toolbar: Reload, Run and Debug the selected task, Jump to Source, a filter. Double-click
 * runs a task as a temporary Rake run configuration (Run / Debug from the Run tool window).
 */
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { Bug, ChevronDown, ChevronRight, FileCode, Folder, Hammer, Play, RefreshCw } from "lucide-solid";
import type { RakeTask } from "@/ipc/ruby";
import type { RunConfiguration } from "@/ipc/run";
import { useTemporary } from "@/features/run/configStore";
import { debugConfiguration, launch } from "@/features/run/runStore";
import { revealSymbol } from "@/features/php/editorViews";
import { rakeLoading, rakeTasks, refreshRake } from "./state";
import { t as tr } from "@/i18n";
import "./ruby.css";

/** The Rake configuration of a task. */
export function rakeConfiguration(t: RakeTask): RunConfiguration {
  return { name: `rake ${t.name}`, type: "rake", options: { task: t.name } };
}

interface Row {
  key: string;
  depth: number;
  label: string;
  task?: RakeTask;
  children: boolean;
}

export function RakeToolWindow() {
  const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
  const [selected, setSelected] = createSignal<string | null>(null);
  const [filter, setFilter] = createSignal("");
  onMount(() => {
    if (!rakeTasks()) void refreshRake();
  });
  const toggle = (k: string) =>
    setCollapsed((s) => {
      const n = new Set(s);
      if (n.has(k)) n.delete(k);
      else n.add(k);
      return n;
    });
  const rows = createMemo<Row[]>(() => {
    const f = filter().trim().toLowerCase();
    const tasks = (rakeTasks()?.tasks ?? []).filter((t) => !f || t.name.toLowerCase().includes(f) || (t.description ?? "").toLowerCase().includes(f));
    const out: Row[] = [];
    const opened = new Set<string>();
    for (const t of tasks) {
      const parts = t.name.split(":");
      // Namespace rows for `a:b:task`.
      let hidden = false;
      for (let i = 0; i < parts.length - 1; i++) {
        const key = `ns:${parts.slice(0, i + 1).join(":")}`;
        if (hidden) break;
        if (!opened.has(key)) {
          opened.add(key);
          out.push({ key, depth: i, label: parts[i], children: true });
        }
        if (collapsed().has(key)) hidden = true;
      }
      if (!hidden) out.push({ key: `task:${t.name}`, depth: parts.length - 1, label: parts[parts.length - 1], task: t, children: false });
    }
    return out;
  });
  const selectedTask = () => rows().find((r) => r.key === selected())?.task;
  const run = (t: RakeTask | undefined) => t && void launch(useTemporary(rakeConfiguration(t)));
  const debug = (t: RakeTask | undefined) => t && void debugConfiguration(useTemporary(rakeConfiguration(t)));
  const jump = (t: RakeTask | undefined) => t?.file && !t.file.startsWith("/") && void revealSymbol(t.file, { line: t.line ?? 0 });
  return (
    <div class="rake-tw">
      <div class="rake-toolbar">
        <button class="icon-btn" title={tr("langs.ruby.reloadRakeTasks")} data-action="rake-reload" onClick={() => void refreshRake(true)}>
          <RefreshCw size={14} classList={{ "rake-spin": rakeLoading() }} />
        </button>
        <span class="rake-sep" />
        <button class="icon-btn" title={tr("langs.ruby.runTask")} disabled={!selectedTask()} onClick={() => run(selectedTask())}>
          <Play size={14} class="rake-run" />
        </button>
        <button class="icon-btn" title={tr("langs.ruby.debugTask")} disabled={!selectedTask()} onClick={() => debug(selectedTask())}>
          <Bug size={14} class="rake-run" />
        </button>
        <button class="icon-btn" title={tr("langs.ruby.jumpToSource")} disabled={!selectedTask()?.file} onClick={() => jump(selectedTask())}>
          <FileCode size={14} />
        </button>
        <input class="rake-filter" type="search" placeholder={tr("langs.ruby.filterTasks")} value={filter()} onInput={(e) => setFilter(e.currentTarget.value)} />
      </div>
      <Show when={rakeTasks()?.error}>
        <div class="rake-message muted" title={rakeTasks()?.error}>
          {rakeTasks()?.source === "files" ? tr("langs.ruby.tasksReadFromTheRakefileLabel") : ""}
          {rakeTasks()?.error}
        </div>
      </Show>
      <Show when={rakeTasks() && !rakeTasks()!.tasks.length}>
        <div class="rake-message muted">{rakeLoading() ? tr("langs.ruby.loadingRakeTasksEllipsis") : tr("langs.ruby.noRakeTasks")}</div>
      </Show>
      <div class="rake-tree" role="tree">
        <For each={rows()}>
          {(r) => (
            <div
              class="tree-row rake-row"
              classList={{ selected: selected() === r.key }}
              style={{ "padding-left": `${6 + r.depth * 16}px` }}
              data-rake-row={r.key}
              title={r.task ? `rake ${r.task.name}${r.task.args?.length ? `[${r.task.args.join(",")}]` : ""}${r.task.file ? `  (${r.task.file})` : ""}` : undefined}
              onClick={() => setSelected(r.key)}
              onDblClick={() => (r.children ? toggle(r.key) : run(r.task))}
            >
              <span class="rake-twisty" onClick={(e) => (e.stopPropagation(), r.children && toggle(r.key))}>
                <Show when={r.children}>{collapsed().has(r.key) ? <ChevronRight size={13} /> : <ChevronDown size={13} />}</Show>
              </span>
              <Show when={r.children} fallback={<Hammer size={14} class="rake-task-icon" />}>
                <Folder size={14} class="rake-ns" />
              </Show>
              <span class="rake-label">{r.label}</span>
              <Show when={r.task?.args?.length}>
                <span class="rake-args">[{r.task!.args!.join(", ")}]</span>
              </Show>
              <Show when={r.task?.description}>
                <span class="muted rake-desc">{r.task!.description}</span>
              </Show>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}
