/**
 * Nx (forge.nx): the Nx tool window with the workspace's projects (applications and
 * libraries), their targets and dependencies (the project graph, read in Rust from
 * `nx.json`, `project.json` and `package.json` files), targets as `nx` run
 * configurations (double-click runs `nx run project:target`) and in Run Anything.
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { Boxes, ChevronDown, ChevronRight, Library, Network, Play, RefreshCw, SquareArrowOutUpRight } from "lucide-solid";
import { nxApi, type NxProject, type NxWorkspace } from "@/ipc/webTooling";
import { registerToolWindow } from "@/features/registry";
import { launch, registerRunConfigurationType } from "@/features/run";
import { registerRunAnythingProvider } from "@/features/runAnything/providers";
import { item, separator, showContextMenu } from "@/features/contextMenu/state";
import { registerCommand } from "@/state/commands";
import { registerKeymapActions } from "@/keymap";
import { toggleToolWindow } from "@/state/layout";
import { openFileAt } from "@/editor/views";
import { isPluginActive } from "@/state/project";
import { t } from "@/i18n";

const active = () => isPluginActive("forge.nx");

const [workspaceData, { refetch }] = createResource(
  () => active(),
  async (on): Promise<NxWorkspace> => (on ? nxApi.workspace() : { projects: [], targetDefaults: [] }),
);
const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
const [view, setView] = createSignal<"projects" | "graph">("projects");

registerRunConfigurationType({
  id: "nx",
  name: "Nx",
  icon: Network,
  color: "var(--info)",
  pluginId: "forge.nx",
  fields: [
    { key: "project", label: "Project", kind: "string", required: true },
    { key: "target", label: "Target", kind: "string", required: true },
    { key: "configuration", label: "Configuration", kind: "string" },
  ],
  suggestName: (cfg) => (cfg.options?.project ? `${cfg.options.project}:${cfg.options?.target ?? ""}` : undefined),
  buildCommand: (cfg) => {
    const o = cfg.options ?? {};
    const target = [o.project, o.target, o.configuration].filter(Boolean).join(":");
    return { program: "npx", args: ["nx", "run", target, ...(cfg.arguments ? cfg.arguments.split(/\s+/).filter(Boolean) : [])], cwd: cfg.workingDirectory ?? "" };
  },
});

export function runNxTarget(project: string, target: string): void {
  void launch({ name: `${project}:${target}`, type: "nx", options: { project, target } });
}

function ProjectNode(props: { p: NxProject }) {
  const open = () => !collapsed().has(props.p.name);
  const toggle = () =>
    setCollapsed((c) => {
      const n = new Set(c);
      if (n.has(props.p.name)) n.delete(props.p.name);
      else n.add(props.p.name);
      return n;
    });
  return (
    <>
      <div class="wt-row" data-nx-project={props.p.name} onClick={toggle}>
        {open() ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
        {props.p.kind === "application" ? <Boxes size={14} class="wt-app" /> : <Library size={14} class="wt-lib" />}
        <span>{props.p.name}</span>
        <span class="muted">{props.p.root}</span>
        <For each={props.p.tags}>{(t) => <span class="wt-badge">{t}</span>}</For>
      </div>
      <Show when={open()}>
        <For each={props.p.targets}>
          {(t) => (
            <div
              class="wt-row wt-child"
              data-nx-target={`${props.p.name}:${t.name}`}
              title={t.executor}
              onDblClick={() => runNxTarget(props.p.name, t.name)}
              onContextMenu={(e) => {
                e.preventDefault();
                showContextMenu(
                  [
                    item(`Run '${props.p.name}:${t.name}'`, () => runNxTarget(props.p.name, t.name)),
                    separator,
                    item("Jump to Source", () => void openFileAt(t.path, t.line + 1)),
                  ],
                  e.clientX,
                  e.clientY,
                );
              }}
            >
              <Play size={12} class="wt-run" />
              <span>{t.name}</span>
              <span class="muted">{t.executor}</span>
            </div>
          )}
        </For>
        <Show when={props.p.dependencies.length}>
          <div class="tree-row wt-row wt-child muted">depends on {props.p.dependencies.join(", ")}</div>
        </Show>
      </Show>
    </>
  );
}

/** The project graph as layers: projects without dependencies first. */
function Graph() {
  const layers = () => {
    const projects = workspaceData()?.projects ?? [];
    const level = new Map<string, number>();
    const depth = (p: NxProject, seen: Set<string>): number => {
      if (level.has(p.name)) return level.get(p.name)!;
      if (seen.has(p.name)) return 0;
      seen.add(p.name);
      const d = Math.max(-1, ...p.dependencies.map((n) => projects.find((x) => x.name === n)).filter(Boolean).map((x) => depth(x!, seen))) + 1;
      level.set(p.name, d);
      return d;
    };
    projects.forEach((p) => depth(p, new Set()));
    const out: NxProject[][] = [];
    for (const p of projects) (out[level.get(p.name) ?? 0] ??= []).push(p);
    return out.filter(Boolean);
  };
  return (
    <div class="nx-graph" data-testid="nx-graph">
      <For each={layers()}>
        {(layer) => (
          <div class="nx-layer">
            <For each={layer}>
              {(p) => (
                <div class="nx-node" classList={{ "nx-app": p.kind === "application" }} title={p.root}>
                  <b>{p.name}</b>
                  <Show when={p.dependencies.length}>
                    <span class="muted">→ {p.dependencies.join(", ")}</span>
                  </Show>
                </div>
              )}
            </For>
          </div>
        )}
      </For>
    </div>
  );
}

function NxView() {
  return (
    <div class="wt-tool" data-testid="nx-window">
      <div class="wt-tabs">
        <button classList={{ active: view() === "projects" }} onClick={() => setView("projects")}>
          Projects
        </button>
        <button classList={{ active: view() === "graph" }} onClick={() => setView("graph")}>
          Project Graph
        </button>
      </div>
      <Show when={(workspaceData()?.projects.length ?? 0) > 0} fallback={<div class="set-list-empty">{workspaceData.loading ? "Reading the workspace…" : "No Nx projects found."}</div>}>
        <Show when={view() === "projects"} fallback={<Graph />}>
          <For each={workspaceData()!.projects}>{(p) => <ProjectNode p={p} />}</For>
        </Show>
      </Show>
    </div>
  );
}

function NxActions() {
  return (
    <>
      <button class="icon-btn" title="Reload Nx Workspace" aria-label="Reload Nx Workspace" onClick={() => void refetch()}>
        <RefreshCw size={14} />
      </button>
      <button class="icon-btn" title="Open nx.json" aria-label="Open nx.json" onClick={() => void openFileAt("nx.json", 1)}>
        <SquareArrowOutUpRight size={14} />
      </button>
    </>
  );
}

registerToolWindow({ id: "nx", title: t("toolwindow.nx.title"), icon: Network, anchor: "left", order: 32, component: NxView, actions: NxActions, when: active });
registerKeymapActions([{ id: "ActivateNxToolWindow", title: "Nx", group: "Tool Windows" }]);
registerCommand({ id: "ActivateNxToolWindow", title: "Nx", category: "View", run: () => toggleToolWindow("nx") });

registerRunAnythingProvider({
  id: "nx",
  title: "Nx targets",
  when: active,
  commands: async () => {
    const ws = workspaceData() ?? (await nxApi.workspace());
    return ws.projects.flatMap((p) => p.targets.map((t) => ({ command: `npx nx run ${p.name}:${t.name}`, description: t.executor })));
  },
});
