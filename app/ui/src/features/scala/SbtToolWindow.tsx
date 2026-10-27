/**
 * The sbt tool window, like IntelliJ IDEA's: the build's projects with their Scala version,
 * tasks, library dependencies and project dependencies, plus the build's sbt plugins. The
 * toolbar reloads the project (sbt prints its model), opens the sbt shell, runs the
 * selected task and expands / collapses the tree.
 *
 * The model comes from Rust (`scala_sbt_model`, `scala_sbt_import`): `build.sbt` and
 * `project/build.properties`, with the last import over it.
 */
import { createMemo, onMount, Show } from "solid-js";
import {
  Boxes,
  ChevronsDownUp,
  ChevronsUpDown,
  CircleX,
  Cog,
  FolderCog,
  Library,
  Link2,
  LoaderCircle,
  Network,
  Play,
  Puzzle,
  RefreshCw,
  Settings,
  Square,
  SquareTerminal,
  X,
} from "lucide-solid";
import { SBT_TASKS, scalaApi, type SbtBuild, type SbtDependency, type SbtProject } from "@/ipc/scala";
import { useTemporary } from "@/features/run/configStore";
import { launch } from "@/features/run/runStore";
import { openSettings } from "@/features/settings/dialogState";
import { BuildTree, branchIds, createTreeState, type TreeNode } from "@/features/java/BuildTree";
import { clearImportError, importBuild, importError, importing, importLog, refreshBuild, sbtBuild } from "./store";
import { SBT_TASK } from "./runTypes";
import "@/features/java/java.css";
import "./scala.css";

/** Run an sbt task of a project (the root project's tasks are not scoped). */
export function runSbtTask(project: SbtProject | null, task: string) {
  const scoped = project && !project.current ? project.id : "";
  const cfg = useTemporary({
    name: scoped ? `${scoped}/${task}` : task,
    type: SBT_TASK,
    options: { tasks: task, project: scoped },
  });
  void launch(cfg);
}

/** The sbt shell in the Run tool window. */
export function openSbtShell() {
  void launch(useTemporary({ name: "sbt shell", type: SBT_TASK, options: { tasks: "" } }));
}

const coordinates = (d: SbtDependency) => `${d.group}${d.cross ? " %% " : ":"}${d.artifact}:${d.version}`;

function dependencyNodes(prefix: string, deps: SbtDependency[]): TreeNode[] {
  return deps.map((d, i) => ({
    id: `${prefix}/${i}`,
    label: coordinates(d),
    secondary: d.configuration ? `(${d.configuration})` : undefined,
    icon: Library,
    iconClass: "java-bt-lib",
    title: `${d.group}:${d.artifact}:${d.version}${d.cross ? " (cross-built for the Scala version)" : ""}`,
    data: { dependency: `${d.group}:${d.artifact}` },
  }));
}

function projectNode(p: SbtProject, b: SbtBuild): TreeNode {
  const id = `sbt:${p.id}`;
  const children: TreeNode[] = [
    {
      id: `${id}/tasks`,
      label: "Tasks",
      icon: FolderCog,
      iconClass: "java-build-icon",
      children: SBT_TASKS.map((t) => ({
        id: `${id}/task/${t.name}`,
        label: t.name,
        icon: Cog,
        title: t.description,
        data: { "sbt-task": t.name },
        onActivate: () => runSbtTask(p, t.name),
      })),
    },
    {
      id: `${id}/deps`,
      label: "Dependencies",
      secondary: b.synced ? undefined : "declared",
      icon: Network,
      iconClass: "java-build-icon",
      data: { "sbt-deps": p.id },
      children: dependencyNodes(`${id}/deps`, p.dependencies),
    },
  ];
  const refs = [...p.dependsOn.map((r) => ({ r, kind: "dependsOn" })), ...p.aggregate.map((r) => ({ r, kind: "aggregates" }))];
  if (refs.length) {
    children.push({
      id: `${id}/refs`,
      label: "Projects",
      icon: Link2,
      iconClass: "java-build-icon",
      children: refs.map(({ r, kind }) => ({
        id: `${id}/refs/${kind}/${r}`,
        label: r,
        secondary: kind === "dependsOn" ? "depends on" : "aggregated",
        icon: Boxes,
        iconClass: "java-build-icon",
      })),
    });
  }
  return {
    id,
    label: p.name,
    secondary: [p.current ? `[${p.id}] root` : p.dir || p.id, p.scalaVersion ? `Scala ${p.scalaVersion}` : ""].filter(Boolean).join(" · "),
    icon: Boxes,
    iconClass: "java-build-icon",
    data: { "sbt-project": p.id },
    children,
  };
}

export function SbtToolWindow() {
  onMount(() => void (sbtBuild() ? undefined : refreshBuild()));
  const tree = createTreeState(["sbt-build"]);
  const nodes = createMemo<TreeNode[]>(() => {
    const b = sbtBuild();
    if (!b) return [];
    const root = b.projects.find((p) => p.current) ?? b.projects[0];
    const projects = [...b.projects].sort((x, y) => Number(y.current) - Number(x.current) || x.id.localeCompare(y.id));
    const plugins: TreeNode[] = b.plugins.length
      ? [
          {
            id: "sbt-plugins",
            label: "sbt plugins",
            icon: Puzzle,
            iconClass: "java-build-icon",
            children: b.plugins.map((p, i) => ({ id: `sbt-plugins/${i}`, label: `${p.group}:${p.artifact}:${p.version}`, icon: Puzzle, iconClass: "java-bt-lib" })),
          },
        ]
      : [];
    return [
      {
        id: "sbt-build",
        label: root?.name ?? "sbt build",
        secondary: [b.sbtVersion ? `sbt ${b.sbtVersion}` : "", b.synced ? "" : "not reloaded yet"].filter(Boolean).join(" · ") || undefined,
        icon: Boxes,
        iconClass: "java-build-icon",
        data: { "sbt-root": "" },
        children: [...projects.map((p) => projectNode(p, b)), ...plugins],
      },
    ];
  });
  const selectedTask = () => {
    const id = tree.selected();
    const b = sbtBuild();
    if (!id?.includes("/task/") || !b) return null;
    const [projectId, task] = id.split("/task/");
    const p = b.projects.find((x) => `sbt:${x.id}` === projectId) ?? null;
    return { p, task: task! };
  };
  const last = () => importLog()[importLog().length - 1]?.line ?? "";
  return (
    <div class="java-build-tw" data-testid="sbt-tool-window">
      <div class="java-toolbar">
        <button class="icon-btn" title="Reload sbt Project" data-testid="sbt-reload" disabled={importing()} onClick={() => void importBuild()}>
          <RefreshCw size={14} />
        </button>
        <button class="icon-btn" title="sbt Shell" data-testid="sbt-shell" onClick={openSbtShell}>
          <SquareTerminal size={14} />
        </button>
        <button class="icon-btn" title="Run Task" disabled={!selectedTask()} onClick={() => selectedTask() && runSbtTask(selectedTask()!.p, selectedTask()!.task)}>
          <Play size={14} />
        </button>
        <span class="java-toolbar-sep" />
        <button class="icon-btn" title="Expand All" onClick={() => tree.setExpanded(new Set(branchIds(nodes(), 4)))}>
          <ChevronsUpDown size={14} />
        </button>
        <button class="icon-btn" title="Collapse All" onClick={() => tree.setExpanded(new Set())}>
          <ChevronsDownUp size={14} />
        </button>
        <span class="java-toolbar-sep" />
        <button class="icon-btn" title="sbt Settings" onClick={() => openSettings("lang.scala")}>
          <Settings size={14} />
        </button>
      </div>
      <Show when={importing()}>
        <div class="java-sync-status" data-testid="sbt-import-status">
          <LoaderCircle size={13} class="lsp-spin" />
          <span class="java-sync-text">sbt: {last() || "starting…"}</span>
          <button class="icon-btn" title="Stop" onClick={() => void scalaApi.sbtCancel()}>
            <Square size={12} />
          </button>
        </div>
      </Show>
      <Show when={!importing() && importError()}>
        <div class="java-sync-status java-sync-failed">
          <CircleX size={13} />
          <span class="java-sync-text" title={importError()!}>{importError()}</span>
          <button class="icon-btn" title="Close" onClick={clearImportError}>
            <X size={12} />
          </button>
        </div>
      </Show>
      <BuildTree nodes={nodes()} state={tree} empty={<div class="muted java-pad">No build.sbt in the project root.</div>} />
    </div>
  );
}
