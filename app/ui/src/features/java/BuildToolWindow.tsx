/**
 * The Gradle and Maven tool windows, like IntelliJ IDEA's: the projects of the build
 * (root, subprojects / modules) with their tasks grouped by group (Gradle) or Lifecycle,
 * Plugins and their goals (Maven), dependency trees per configuration / with scopes,
 * Gradle run configurations and Maven profiles with checkboxes. The toolbars reload
 * (sync), execute a task or goal, toggle offline mode and Skip Tests, expand / collapse,
 * show the dependencies and open the settings.
 *
 * The model comes from Rust (`java_build_model`, `java_sync_project`): the build files'
 * data, with the tasks and resolved dependencies of the last sync over it.
 */
import { createMemo, For, onMount, Show } from "solid-js";
import {
  Boxes,
  ChevronsDownUp,
  ChevronsUpDown,
  CircleX,
  Cog,
  FlaskConicalOff,
  Folder,
  FolderCog,
  Library,
  ListTree,
  LoaderCircle,
  Network,
  Package,
  Play,
  Puzzle,
  RefreshCw,
  Settings,
  SquareTerminal,
  Square,
  WifiOff,
  X,
} from "lucide-solid";
import type { BuildModel, DependencyNode, ProjectModel } from "@/ipc/java";
import { allConfigurations } from "@/features/run/configStore";
import { runConfiguration } from "@/features/run/runStore";
import { openSettings } from "@/features/settings/dialogState";
import { getSetting, saveSetting } from "@/state/settings";
import { javaApi } from "@/ipc/java";
import { buildModel, clearSyncLog, refreshModel, syncError, syncing, syncLog, syncProject } from "./store";
import { BuildTree, branchIds, createTreeState, findNode, type TreeNode } from "./BuildTree";
import { openExecuteDialog } from "./ExecuteDialog";
import { gradleTask, runGradle, runMaven } from "./actions";
import "./java.css";

// ---- Shared parts ----------------------------------------------------------------------------

function dependencyNodes(prefix: string, deps: DependencyNode[], maven: boolean): TreeNode[] {
  return deps.map((d, i) => {
    const id = `${prefix}/${i}`;
    const project = d.id.startsWith("project ");
    let secondary = "";
    if (maven && d.scope) secondary = `(${d.scope})`;
    if (d.requested) secondary = `${secondary} ${d.requested} → ${d.version}`.trim();
    if (d.omitted) secondary = `${secondary} (*)`.trim();
    return {
      id,
      label: project ? d.id : `${d.group}:${d.artifact}${d.version ? `:${d.version}` : ""}`,
      secondary: secondary || undefined,
      icon: d.failed ? CircleX : project ? Boxes : Library,
      iconClass: d.failed ? "java-bt-error" : project ? "java-build-icon" : "java-bt-lib",
      tone: d.failed ? "danger" : d.omitted ? "muted" : undefined,
      title: d.failed ? `Could not resolve ${d.id}` : d.omitted ? `${d.id} (listed above)` : d.id,
      children: dependencyNodes(id, d.children, maven),
      data: { dependency: d.id },
    } satisfies TreeNode;
  });
}

function declaredNodes(prefix: string, p: ProjectModel): TreeNode[] {
  return p.declared.map((d, i) => ({
    id: `${prefix}/declared/${i}`,
    label: `${d.group}:${d.artifact}${d.version ? `:${d.version}` : ""}`,
    secondary: `(${d.configuration})`,
    icon: Library,
    iconClass: "java-bt-lib",
    title: "Declared in the build file; Reload shows the resolved tree",
  }));
}

const ago = (secs?: number) => {
  if (!secs) return "";
  const d = Math.max(0, Math.round(Date.now() / 1000 - secs));
  if (d < 60) return "just now";
  if (d < 3600) return `${Math.round(d / 60)} min ago`;
  if (d < 86400) return `${Math.round(d / 3600)} h ago`;
  return new Date(secs * 1000).toLocaleDateString();
};

/** Sync progress, the last result and its output. */
function SyncPanel(props: { tool: string }) {
  const last = () => syncLog()[syncLog().length - 1]?.line ?? "";
  return (
    <>
      <Show when={syncing()}>
        <div class="java-sync-status" data-testid="java-sync-status">
          <LoaderCircle size={13} class="lsp-spin" />
          <span class="java-sync-text">{props.tool} sync: {last() || "starting…"}</span>
          <button class="icon-btn" title="Stop" onClick={() => void javaApi.syncCancel()}>
            <Square size={12} />
          </button>
        </div>
      </Show>
      <Show when={!syncing() && syncError()}>
        <div class="java-sync-status java-sync-failed" data-testid="java-sync-error">
          <CircleX size={13} />
          <span class="java-sync-text" title={syncError()!}>{syncError()}</span>
          <button class="icon-btn" title="Close" onClick={clearSyncLog}>
            <X size={12} />
          </button>
        </div>
        <Show when={syncLog().length}>
          <pre class="java-sync-log">{syncLog().slice(-40).map((l) => l.line).join("\n")}</pre>
        </Show>
      </Show>
    </>
  );
}

function Toggle(props: { on: boolean; title: string; icon: typeof WifiOff; onClick: () => void; testid?: string }) {
  return (
    <button class="icon-btn" classList={{ active: props.on }} aria-pressed={props.on} title={props.title} data-testid={props.testid} onClick={props.onClick}>
      <props.icon size={14} />
    </button>
  );
}

function useModel() {
  onMount(() => void (buildModel() ? undefined : refreshModel()));
  return buildModel;
}

const empty = (tool: string) => <div class="muted java-pad">No {tool} build in the project root.</div>;

// ---- Gradle ----------------------------------------------------------------------------------

function gradleProjectNode(p: ProjectModel, m: BuildModel, children: TreeNode[]): TreeNode {
  const id = `p:${p.path}`;
  const groups = new Map<string, ProjectModel["tasks"]>();
  for (const t of p.tasks) groups.set(t.group || "other", [...(groups.get(t.group || "other") ?? []), t]);
  const order = (g: string) => (g === "other" ? "zz" : g.toLowerCase());
  const tasks: TreeNode = {
    id: `${id}/tasks`,
    label: "Tasks",
    icon: FolderCog,
    iconClass: "java-build-icon",
    children: [...groups.entries()]
      .sort((a, b) => order(a[0]).localeCompare(order(b[0])))
      .map(([g, list]) => ({
        id: `${id}/tasks/${g}`,
        label: g,
        icon: Folder,
        iconClass: "java-build-icon",
        data: { "build-group": g },
        children: list.map((t) => ({
          id: `${id}/task/${t.name}`,
          label: t.name,
          icon: Cog,
          title: t.description || t.name,
          data: { goal: t.name },
          onActivate: () => runGradle(gradleTask(p, t.name)),
        })),
      })),
  };
  const deps: TreeNode = {
    id: `${id}/deps`,
    label: "Dependencies",
    icon: Network,
    iconClass: "java-build-icon",
    data: { "build-deps": p.path },
    children: m.synced
      ? p.dependencies.map((c) => ({
          id: `${id}/deps/${c.name}`,
          label: c.name,
          secondary: c.description || undefined,
          icon: ListTree,
          iconClass: "java-build-icon",
          data: { configuration: c.name },
          children: dependencyNodes(`${id}/deps/${c.name}`, c.dependencies, false),
        }))
      : declaredNodes(id, p),
  };
  return {
    id,
    label: p.name,
    secondary: p.path === ":" && m.gradleVersion ? `Gradle ${m.gradleVersion}` : undefined,
    icon: Boxes,
    iconClass: "java-build-icon",
    data: { "build-project": p.path },
    children: [tasks, deps, ...children],
  };
}

export function GradleToolWindow() {
  const model = useModel();
  const tree = createTreeState(["p::", "p::/tasks", "p::/deps"]);
  const offline = () => getSetting<boolean>("java.gradle.offline", false);
  const nodes = createMemo<TreeNode[]>(() => {
    const m = model();
    if (!m || m.tool !== "gradle") return [];
    const [root, ...subs] = m.projects;
    if (!root) return [];
    const runConfigs: TreeNode = {
      id: "p::/runs",
      label: "Run Configurations",
      icon: Folder,
      iconClass: "java-build-icon",
      children: allConfigurations()
        .filter((c) => c.type === "gradle")
        .map((c) => ({
          id: `run:${c.name}`,
          label: c.name,
          secondary: c.options?.tasks,
          icon: Play,
          iconClass: "java-ok",
          onActivate: () => void runConfiguration(c),
        })),
    };
    const children = subs.map((s) => gradleProjectNode(s, m, []));
    return [gradleProjectNode(root, m, [...(runConfigs.children!.length ? [runConfigs] : []), ...children])];
  });
  const selectedTask = () => {
    const id = tree.selected();
    const m = model();
    if (!id?.includes("/task/") || !m) return null;
    const [projectId, name] = id.split("/task/");
    const p = m.projects.find((x) => `p:${x.path}` === projectId);
    return p ? gradleTask(p, name!) : null;
  };
  const showDependencies = () => {
    const sel = tree.selected();
    const project = sel?.match(/^(p:[^/]*)/)?.[1] ?? "p::";
    const deps = findNode(nodes(), `${project}/deps`);
    tree.setExpanded(new Set([...tree.expanded(), project, `${project}/deps`, ...branchIds(deps?.children ?? [], 1)]));
    tree.setSelected(`${project}/deps`);
  };
  return (
    <div class="java-build-tw" data-testid="gradle-tool-window">
      <div class="java-toolbar">
        <button class="icon-btn" title="Reload All Gradle Projects" data-testid="gradle-sync" disabled={syncing()} onClick={() => void syncProject()}>
          <RefreshCw size={14} />
        </button>
        <button class="icon-btn" title="Execute Gradle Task…" data-testid="gradle-execute" onClick={() => openExecuteDialog()}>
          <SquareTerminal size={14} />
        </button>
        <button class="icon-btn" title="Run" disabled={!selectedTask()} onClick={() => selectedTask() && runGradle(selectedTask()!)}>
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
        <Toggle on={offline()} title="Toggle Offline Mode" icon={WifiOff} testid="gradle-offline" onClick={() => void saveSetting("java.gradle.offline", !offline())} />
        <button class="icon-btn" title="Show Dependencies" onClick={showDependencies}>
          <Network size={14} />
        </button>
        <button class="icon-btn" title="Gradle Settings" onClick={() => openSettings("build.tools.gradle")}>
          <Settings size={14} />
        </button>
      </div>
      <SyncPanel tool="Gradle" />
      <Show when={model()?.tool === "gradle" && !model()!.synced && !syncing()}>
        <div class="java-sync-hint muted">
          Tasks and dependencies from the build files. <button class="link-btn" onClick={() => void syncProject()}>Reload</button> asks Gradle for all of them.
        </div>
      </Show>
      <Show when={model()?.synced && model()?.syncedAt}>
        <div class="java-sync-hint muted">Synced {ago(model()!.syncedAt)}{offline() ? " · offline" : ""}</div>
      </Show>
      <BuildTree nodes={nodes()} state={tree} empty={empty("Gradle")} />
    </div>
  );
}

// ---- Maven -------------------------------------------------------------------------------------

/** The selected profiles (`id` activates, `!id` deactivates). */
const selectedProfiles = () => getSetting<string[]>("java.maven.profiles", []) ?? [];

function toggleProfile(id: string, activeByDefault: boolean) {
  const list = selectedProfiles().filter((p) => p !== id && p !== `!${id}`);
  const was = selectedProfiles();
  const checked = was.includes(id) || (activeByDefault && !was.includes(`!${id}`));
  if (checked && activeByDefault) list.push(`!${id}`);
  else if (!checked && !activeByDefault) list.push(id);
  void saveSetting("java.maven.profiles", list);
}

function mavenProjectNode(p: ProjectModel, m: BuildModel, children: TreeNode[]): TreeNode {
  const id = `m:${p.dir}`;
  const lifecycle: TreeNode = {
    id: `${id}/lifecycle`,
    label: "Lifecycle",
    icon: FolderCog,
    iconClass: "java-build-icon",
    children: p.tasks
      .filter((t) => t.group === "Lifecycle")
      .map((t) => ({
        id: `${id}/goal/${t.name}`,
        label: t.name,
        icon: Cog,
        title: t.description,
        data: { goal: t.name },
        onActivate: () => runMaven(t.name, p.dir),
      })),
  };
  const plugins: TreeNode = {
    id: `${id}/plugins`,
    label: "Plugins",
    icon: Folder,
    iconClass: "java-build-icon",
    children: p.plugins.map((pl) => ({
      id: `${id}/plugin/${pl.prefix}`,
      label: pl.prefix,
      secondary: `(${pl.id}${pl.version ? `:${pl.version}` : ""})`,
      icon: Puzzle,
      iconClass: "java-build-icon",
      children: pl.goals.map((g) => ({
        id: `${id}/goal/${pl.prefix}:${g}`,
        label: `${pl.prefix}:${g}`,
        icon: Cog,
        data: { goal: `${pl.prefix}:${g}` },
        onActivate: () => runMaven(`${pl.prefix}:${g}`, p.dir),
      })),
    })),
  };
  const deps: TreeNode = {
    id: `${id}/deps`,
    label: "Dependencies",
    icon: Network,
    iconClass: "java-build-icon",
    data: { "build-deps": p.dir || "root" },
    children: m.synced ? dependencyNodes(`${id}/deps`, p.dependencies[0]?.dependencies ?? [], true) : declaredNodes(id, p),
  };
  return {
    id,
    label: p.name,
    icon: Package,
    iconClass: "java-build-icon",
    data: { "build-project": p.dir || "root" },
    children: [lifecycle, plugins, deps, ...children],
  };
}

export function MavenToolWindow() {
  const model = useModel();
  const tree = createTreeState(["profiles", "m:", "m:/lifecycle"]);
  const offline = () => getSetting<boolean>("java.maven.offline", false);
  const skipTests = () => getSetting<boolean>("java.maven.skipTests", false);
  const nodes = createMemo<TreeNode[]>(() => {
    const m = model();
    if (!m || m.tool !== "maven") return [];
    const out: TreeNode[] = [];
    if (m.profiles.length) {
      const sel = selectedProfiles();
      out.push({
        id: "profiles",
        label: "Profiles",
        icon: Folder,
        iconClass: "java-build-icon",
        children: m.profiles.map((pr) => ({
          id: `profile:${pr.id}`,
          label: pr.id,
          secondary: pr.activeByDefault ? "active by default" : undefined,
          icon: Cog,
          data: { profile: pr.id },
          checkbox: {
            checked: sel.includes(pr.id) || (pr.activeByDefault && !sel.includes(`!${pr.id}`)),
            onToggle: () => toggleProfile(pr.id, pr.activeByDefault),
          },
        })),
      });
    }
    const [root, ...modules] = m.projects;
    if (root) out.push(mavenProjectNode(root, m, modules.map((x) => mavenProjectNode(x, m, []))));
    return out;
  });
  const selectedGoal = () => {
    const id = tree.selected();
    if (!id?.includes("/goal/")) return null;
    const [project, goal] = id.split("/goal/");
    return { goal: goal!, module: project!.slice(2) };
  };
  const showDependencies = () => {
    const project = tree.selected()?.match(/^(m:[^/]*)/)?.[1] ?? "m:";
    tree.setExpanded(new Set([...tree.expanded(), project, `${project}/deps`]));
    tree.setSelected(`${project}/deps`);
  };
  return (
    <div class="java-build-tw" data-testid="maven-tool-window">
      <div class="java-toolbar">
        <button class="icon-btn" title="Reload All Maven Projects" data-testid="maven-sync" disabled={syncing()} onClick={() => void syncProject()}>
          <RefreshCw size={14} />
        </button>
        <button class="icon-btn" title="Execute Maven Goal…" data-testid="maven-execute" onClick={() => openExecuteDialog()}>
          <SquareTerminal size={14} />
        </button>
        <button class="icon-btn" title="Run Maven Build" disabled={!selectedGoal()} onClick={() => selectedGoal() && runMaven(selectedGoal()!.goal, selectedGoal()!.module)}>
          <Play size={14} />
        </button>
        <span class="java-toolbar-sep" />
        <Toggle on={offline()} title="Toggle Offline Mode" icon={WifiOff} testid="maven-offline" onClick={() => void saveSetting("java.maven.offline", !offline())} />
        <Toggle on={skipTests()} title="Toggle 'Skip Tests' Mode" icon={FlaskConicalOff} testid="maven-skip-tests" onClick={() => void saveSetting("java.maven.skipTests", !skipTests())} />
        <span class="java-toolbar-sep" />
        <button class="icon-btn" title="Expand All" onClick={() => tree.setExpanded(new Set(branchIds(nodes(), 4)))}>
          <ChevronsUpDown size={14} />
        </button>
        <button class="icon-btn" title="Collapse All" onClick={() => tree.setExpanded(new Set())}>
          <ChevronsDownUp size={14} />
        </button>
        <button class="icon-btn" title="Show Dependencies" onClick={showDependencies}>
          <Network size={14} />
        </button>
        <button class="icon-btn" title="Maven Settings" onClick={() => openSettings("build.tools.maven")}>
          <Settings size={14} />
        </button>
      </div>
      <SyncPanel tool="Maven" />
      <Show when={model()?.tool === "maven" && !model()!.synced && !syncing()}>
        <div class="java-sync-hint muted">
          Declared dependencies only. <button class="link-btn" onClick={() => void syncProject()}>Reload</button> resolves the dependency tree.
        </div>
      </Show>
      <Show when={model()?.synced && model()?.syncedAt}>
        <div class="java-sync-hint muted">
          Synced {ago(model()!.syncedAt)}
          <For each={[offline() && "offline", skipTests() && "skipping tests"].filter(Boolean)}>{(t) => <> · {t}</>}</For>
        </div>
      </Show>
      <BuildTree nodes={nodes()} state={tree} empty={empty("Maven")} />
    </div>
  );
}
