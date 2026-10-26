/**
 * A small Gradle / Maven tool window: the build's tasks (Gradle, grouped) or lifecycle
 * phases and plugin goals (Maven), from `java_build_info`. Double-click runs one as a
 * temporary Gradle / Maven configuration; Reload re-imports the build through JDT LS. The
 * full tool windows (dependencies, sync, offline mode) are a later roadmap item.
 */
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { ChevronDown, ChevronRight, Cog, Folder, Play, RefreshCw } from "lucide-solid";
import type { Goal } from "@/ipc/java";
import { useTemporary } from "@/features/run/configStore";
import { launch } from "@/features/run/runStore";
import { buildView, refreshBuild } from "./store";
import { reloadProject } from "./actions";
import "./java.css";

function runGoal(goal: Goal) {
  const b = buildView();
  if (!b) return;
  const gradle = b.project.tool === "gradle";
  const cfg = useTemporary({
    name: `${b.project.name} [${goal.name}]`,
    type: gradle ? "gradle" : "maven",
    options: gradle ? { tasks: goal.name } : { goals: goal.name },
  });
  void launch(cfg);
}

export function BuildToolWindow() {
  const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
  const [selected, setSelected] = createSignal<string | null>(null);
  onMount(() => void refreshBuild());
  const groups = createMemo(() => {
    const out = new Map<string, Goal[]>();
    for (const g of buildView()?.goals ?? []) out.set(g.group, [...(out.get(g.group) ?? []), g]);
    return [...out.entries()];
  });
  const toggle = (key: string) =>
    setCollapsed((s) => {
      const n = new Set(s);
      if (n.has(key)) n.delete(key);
      else n.add(key);
      return n;
    });
  const selectedGoal = () => buildView()?.goals.find((g) => `${g.group}/${g.name}` === selected());
  return (
    <div class="java-build-tw">
      <div class="java-toolbar">
        <button class="icon-btn" title="Reload All Projects" onClick={() => void reloadProject()}>
          <RefreshCw size={14} />
        </button>
        <button class="icon-btn" title="Run" disabled={!selectedGoal()} onClick={() => selectedGoal() && runGoal(selectedGoal()!)}>
          <Play size={14} />
        </button>
      </div>
      <Show when={buildView()} fallback={<div class="muted java-pad">No Maven or Gradle build in the project root.</div>}>
        {(b) => (
          <div class="java-build-tree">
            <div class="tree-row java-tree-root">
              <Folder size={14} class="java-build-icon" />
              <span>{b().project.name}</span>
              <span class="muted">{b().project.tool === "gradle" ? (b().project.wrapper ? "gradlew" : "gradle") : b().project.wrapper ? "mvnw" : "mvn"}</span>
            </div>
            <For each={groups()}>
              {([group, goals]) => (
                <>
                  <div class="tree-row java-tree-group" onClick={() => toggle(group)} data-build-group={group}>
                    {collapsed().has(group) ? <ChevronRight size={13} /> : <ChevronDown size={13} />}
                    <Folder size={13} class="java-build-icon" />
                    <span>{group}</span>
                  </div>
                  <Show when={!collapsed().has(group)}>
                    <For each={goals}>
                      {(g) => (
                        <div
                          class="tree-row java-tree-goal"
                          classList={{ selected: selected() === `${group}/${g.name}` }}
                          title={g.description}
                          data-goal={g.name}
                          onClick={() => setSelected(`${group}/${g.name}`)}
                          onDblClick={() => runGoal(g)}
                        >
                          <Cog size={13} class="muted" />
                          <span>{g.name}</span>
                        </div>
                      )}
                    </For>
                  </Show>
                </>
              )}
            </For>
          </div>
        )}
      </Show>
    </div>
  );
}
