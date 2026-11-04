/**
 * The Build tool window, like Rider's: the solution on the left (solution folders, projects
 * with their target frameworks, launch profiles, test projects), the last build on the right
 * (summary, then the compiler, NuGet and MSBuild problems grouped by file, parsed in Rust
 * from the `dotnet build` output, `dotnet_build_problems`), or the raw build output.
 *
 * Toolbar: Build, Rebuild, Clean and Restore the solution or the selected project, Reload
 * the solution, the build configuration (Debug / Release) and the active solution.
 * Double-click runs a project, opens a problem's location.
 */
import { createMemo, createSignal, For, onMount, Show, type JSX } from "solid-js";
import { Dynamic } from "solid-js/web";
import {
  AppWindow,
  Bug,
  ChevronDown,
  ChevronRight,
  CircleCheck,
  CircleX,
  Eraser,
  FileCode,
  FlaskConical,
  Folder,
  Globe,
  Hammer,
  Library,
  Package,
  Play,
  RefreshCw,
  ScrollText,
  Wrench,
} from "lucide-solid";
import type { BuildProblem, DotnetAction, DotnetProject } from "@/ipc/dotnet";
import type { RunConfiguration } from "@/ipc/run";
import { severityIcon } from "@/features/lsp/severity";
import { useTemporary } from "@/features/run/configStore";
import { debugConfiguration, launch } from "@/features/run/runStore";
import { openFileAt } from "@/editor/views";
import { getSetting, saveSettings } from "@/state/settings";
import {
  buildReport,
  dotnetBuildOutput,
  dotnetBuilding,
  dotnetLoading,
  dotnetWorkspace,
  dotnetWorkspaceError,
  refreshSolution,
  runBuild,
} from "./state";
import { t } from "@/i18n";
import "./dotnet.css";

type Icon = (p: { size?: number; class?: string }) => JSX.Element;

interface Row {
  key: string;
  depth: number;
  label: string;
  detail?: string;
  icon: Icon;
  iconClass: string;
  children: boolean;
  project?: DotnetProject;
  profile?: string;
}

function projectIcon(p: DotnetProject): Icon {
  if (p.isTestProject) return FlaskConical;
  if (p.web) return Globe;
  if (/exe$/i.test(p.outputType)) return AppWindow;
  return Library;
}

const languageBadge = (p: DotnetProject) => (p.language === "fSharp" ? "F#" : p.language === "visualBasic" ? "VB" : "C#");

/** The configuration that runs a project (with a launch profile). */
export function runConfigurationFor(p: DotnetProject, profile?: string): RunConfiguration {
  return {
    name: profile ? `${p.name}: ${profile}` : p.name,
    type: "dotnet",
    options: { project: p.path, ...(profile ? { profile } : {}) },
  };
}

function testConfigurationFor(p: DotnetProject): RunConfiguration {
  return { name: `Tests in ${p.name}`, type: "dotnet-test", options: { scope: "project", project: p.path } };
}

const runnable = (p: DotnetProject) => !p.isTestProject && (p.web || /exe$/i.test(p.outputType));

function SolutionTree(props: { selected: string | null; onSelect: (key: string, project?: DotnetProject) => void }) {
  // Folders, projects and profiles start open, package lists closed.
  const [toggled, setToggled] = createSignal<Set<string>>(new Set());
  const toggle = (k: string) =>
    setToggled((s) => {
      const n = new Set(s);
      if (n.has(k)) n.delete(k);
      else n.add(k);
      return n;
    });
  const isOpen = (k: string) => (k.endsWith("/packages") ? toggled().has(k) : !toggled().has(k));
  const rows = createMemo<Row[]>(() => {
    const ws = dotnetWorkspace();
    if (!ws) return [];
    const out: Row[] = [];
    const sln = ws.solutions.find((s) => s.path === ws.solution);
    const byPath = new Map(ws.projects.map((p) => [p.path, p]));
    const pushProject = (p: DotnetProject, depth: number) => {
      const k = `p:${p.path}`;
      const profiles = (p.launchProfiles ?? []).filter((x) => x.commandName === "Project" || x.commandName === "Executable");
      out.push({
        key: k,
        depth,
        label: p.name,
        detail: [languageBadge(p), p.targetFrameworks.join(", ")].filter(Boolean).join("  "),
        icon: projectIcon(p),
        iconClass: p.isTestProject ? "dn-test" : runnable(p) ? "dn-app" : "dn-lib",
        children: profiles.length > 0 || p.packages.length > 0,
        project: p,
      });
      if (!isOpen(k)) return;
      for (const pr of profiles)
        out.push({ key: `${k}/profile:${pr.name}`, depth: depth + 1, label: pr.name, detail: pr.applicationUrl?.split(";")[0] ?? "launch profile", icon: Play, iconClass: "dn-app", children: false, project: p, profile: pr.name });
      if (p.packages.length) {
        const pk = `${k}/packages`;
        out.push({ key: pk, depth: depth + 1, label: t("langs.dotnet.packages"), icon: Package, iconClass: "muted", children: true, project: p });
        if (isOpen(pk))
          for (const r of p.packages)
            out.push({ key: `${pk}/${r.id}`, depth: depth + 2, label: r.id, detail: r.version || "implicit", icon: Package, iconClass: "dn-pkg", children: false, project: p });
      }
    };
    if (sln) {
      const rootKey = `s:${sln.path}`;
      out.push({ key: rootKey, depth: 0, label: t("langs.dotnet.solution", { name: sln.name }), detail: t("langs.dotnet.projects", { count: sln.projects.length }), icon: FileCode, iconClass: "dn-sln", children: true });
      if (!isOpen(rootKey)) return out;
      const walk = (folder: string, depth: number) => {
        for (const f of sln.folders.filter((x) => (x.includes("/") ? x.slice(0, x.lastIndexOf("/")) : "") === folder)) {
          const fk = `f:${f}`;
          out.push({ key: fk, depth, label: f.split("/").pop()!, icon: Folder, iconClass: "dn-folder", children: true });
          if (isOpen(fk)) walk(f, depth + 1);
        }
        for (const sp of sln.projects.filter((x) => (x.folder ?? "") === folder)) {
          const p = byPath.get(sp.path);
          if (p) pushProject(p, depth);
          else out.push({ key: `m:${sp.path}`, depth, label: sp.name, detail: "not loaded", icon: CircleX, iconClass: "dn-error", children: false });
        }
      };
      walk("", 1);
    } else {
      for (const p of ws.projects) pushProject(p, 0);
    }
    return out;
  });
  const run = (cfg: RunConfiguration) => void launch(useTemporary(cfg));
  return (
    <div class="dn-tree" role="tree" data-testid="dotnet-solution-tree">
      <For each={rows()}>
        {(r) => (
          <div
            class="tree-row dn-row"
            classList={{ selected: props.selected === r.key }}
            style={{ "padding-left": `${6 + r.depth * 16}px` }}
            data-dn-row={r.key}
            title={r.project?.path}
            onClick={() => props.onSelect(r.key, r.project)}
            onDblClick={() => {
              if (r.profile && r.project) run(runConfigurationFor(r.project, r.profile));
              else if (r.children) toggle(r.key);
              else if (r.project && r.key.startsWith("p:") && runnable(r.project)) run(runConfigurationFor(r.project));
              else if (r.project) void openFileAt(r.project.path);
            }}
          >
            <span class="dn-twisty" onClick={(e) => (e.stopPropagation(), r.children && toggle(r.key))}>
              <Show when={r.children}>{isOpen(r.key) ? <ChevronDown size={13} /> : <ChevronRight size={13} />}</Show>
            </span>
            <Dynamic component={r.icon} size={14} class={`dn-icon ${r.iconClass}`} />
            <span class="dn-label">{r.label}</span>
            <Show when={r.detail}>
              <span class="muted dn-detail">{r.detail}</span>
            </Show>
          </div>
        )}
      </For>
    </div>
  );
}

function place(p: BuildProblem): string {
  const name = (p.path ?? "").split("/").pop();
  if (!name) return p.origin || p.project?.split("/").pop() || "";
  return p.line || p.column ? `${name}:${p.line + 1}:${p.column + 1}` : name;
}

function reveal(p: BuildProblem): void {
  if (!p.path || p.path.startsWith("/") || /^[A-Za-z]:/.test(p.path)) return;
  void openFileAt(p.path, p.line + 1, p.column + 1);
}

function BuildResults(props: { showOutput: boolean }) {
  const groups = createMemo(() => {
    const r = buildReport();
    const map = new Map<string, BuildProblem[]>();
    for (const p of r?.problems ?? []) {
      const k = p.path || p.project || p.origin || "";
      map.set(k, [...(map.get(k) ?? []), p]);
    }
    return [...map.entries()];
  });
  const summary = () => {
    const r = buildReport();
    if (!r) return "";
    const counts = `${r.errors} error${r.errors === 1 ? "" : "s"}, ${r.warnings} warning${r.warnings === 1 ? "" : "s"}`;
    const time = r.elapsed ? ` in ${r.elapsed.replace(/^00:/, "")}` : "";
    return r.succeeded === false ? `Build failed: ${counts}${time}` : r.succeeded ? `Build succeeded: ${counts}${time}` : `Build stopped: ${counts}`;
  };
  return (
    <div class="dn-results">
      <Show when={dotnetBuilding()}>
        <div class="dn-summary">
          <Hammer size={13} class="dn-spin-slow" />
          <span>{dotnetBuilding()}…</span>
        </div>
      </Show>
      <Show when={!dotnetBuilding() && buildReport()}>
        <div class="dn-summary" data-testid="dotnet-build-summary">
          <Show when={buildReport()!.succeeded === false} fallback={<CircleCheck size={13} class="dn-ok" />}>
            <CircleX size={13} class="dn-error" />
          </Show>
          <span>{summary()}</span>
        </div>
      </Show>
      <Show
        when={!props.showOutput}
        fallback={
          <pre class="dn-output" data-testid="dotnet-build-output">
            {dotnetBuildOutput() || "No build output yet"}
          </pre>
        }
      >
        <div class="dn-problems problems-tree" data-testid="dotnet-build-problems">
          <Show when={buildReport()} fallback={<div class="problems-empty">{t("langs.dotnet.buildTheSolutionCtrlF9")}</div>}>
            <Show when={groups().length} fallback={<div class="problems-empty">{t("langs.dotnet.noErrorsOrWarnings")}</div>}>
              <For each={groups()}>
                {([file, list]) => (
                  <>
                    <div class="tree-row dn-file-row" title={file}>
                      <FileCode size={14} class="dn-icon muted" />
                      <span class="dn-label">{file.split("/").pop() || t("langs.dotnet.build")}</span>
                      <span class="muted dn-detail">{file.includes("/") ? file.slice(0, file.lastIndexOf("/")) : ""}</span>
                      <span class="muted dn-detail">
                        {t("langs.dotnet.problems", { count: list.length })}
                      </span>
                    </div>
                    <For each={list}>
                      {(p) => (
                        <div
                          class="tree-row problem-row dn-problem"
                          tabIndex={0}
                          data-testid="dotnet-build-problem"
                          onClick={() => reveal(p)}
                          onKeyDown={(e) => e.key === "Enter" && reveal(p)}
                        >
                          <span class={`problem-icon sev-${p.severity}`}>
                            <Dynamic component={severityIcon[p.severity]} size={14} />
                          </span>
                          <span class="problem-message">{p.message}</span>
                          <Show when={p.code}>
                            <span class="tree-hint">{p.code}</span>
                          </Show>
                          <span class="tree-hint">{place(p)}</span>
                        </div>
                      )}
                    </For>
                  </>
                )}
              </For>
            </Show>
          </Show>
        </div>
      </Show>
    </div>
  );
}

export function BuildToolWindow() {
  const [selected, setSelected] = createSignal<string | null>(null);
  const [project, setProject] = createSignal<DotnetProject | undefined>();
  const [showOutput, setShowOutput] = createSignal(false);
  onMount(() => {
    if (!dotnetWorkspace()) void refreshSolution();
  });
  const action = (a: DotnetAction) => void runBuild(a, project()?.path);
  const ws = () => dotnetWorkspace();
  const configuration = () => getSetting<string>("dotnet.configuration", "Debug") || "Debug";
  const selectedRun = () => {
    const p = project();
    if (!p) return null;
    const profile = selected()?.match(/\/profile:(.+)$/)?.[1];
    if (p.isTestProject) return testConfigurationFor(p);
    return runnable(p) ? runConfigurationFor(p, profile) : null;
  };
  return (
    <div class="dn-tw" data-testid="dotnet-build-window">
      <div class="dn-toolbar">
        <button class="icon-btn" title={project() ? `Build ${project()!.name}` : t("langs.dotnet.buildSolution")} data-action="dotnet-build" disabled={!!dotnetBuilding()} onClick={() => action("build")}>
          <Hammer size={14} />
        </button>
        <button class="icon-btn" title={project() ? `Rebuild ${project()!.name}` : t("langs.dotnet.rebuildSolution")} disabled={!!dotnetBuilding()} onClick={() => action("rebuild")}>
          <Wrench size={14} />
        </button>
        <button class="icon-btn" title={project() ? `Clean ${project()!.name}` : t("langs.dotnet.cleanSolution")} disabled={!!dotnetBuilding()} onClick={() => action("clean")}>
          <Eraser size={14} />
        </button>
        <button class="icon-btn" title={t("langs.dotnet.restoreNugetPackages")} disabled={!!dotnetBuilding()} onClick={() => action("restore")}>
          <Package size={14} />
        </button>
        <span class="dn-sep" />
        <button class="icon-btn" title={t("common.run2")} disabled={!selectedRun()} onClick={() => selectedRun() && void launch(useTemporary(selectedRun()!))}>
          <Play size={14} class="dn-run" />
        </button>
        <button class="icon-btn" title={t("common.debug2")} disabled={!selectedRun()} onClick={() => selectedRun() && void debugConfiguration(useTemporary(selectedRun()!))}>
          <Bug size={14} class="dn-run" />
        </button>
        <span class="dn-sep" />
        <button class="icon-btn" title={t("langs.dotnet.reloadSolution")} data-action="dotnet-reload" onClick={() => void refreshSolution(true)}>
          <RefreshCw size={14} classList={{ "dn-spin": dotnetLoading() }} />
        </button>
        <button class="icon-btn" classList={{ active: showOutput() }} title={t("langs.dotnet.showBuildOutput")} data-action="dotnet-output" onClick={() => setShowOutput((v) => !v)}>
          <ScrollText size={14} />
        </button>
        <span class="dn-spacer" />
        <Show when={(ws()?.solutions.length ?? 0) > 1}>
          <select
            class="dn-select"
            title={t("langs.dotnet.solution2")}
            value={ws()?.solution ?? ""}
            onChange={(e) => void saveSettings({ "dotnet.solution": e.currentTarget.value }).then(() => refreshSolution(true))}
          >
            <For each={ws()?.solutions ?? []}>{(s) => <option value={s.path}>{s.path}</option>}</For>
          </select>
        </Show>
        <select
          class="dn-select"
          title={t("langs.dotnet.buildConfiguration")}
          data-testid="dotnet-configuration"
          value={configuration()}
          onChange={(e) => void saveSettings({ "dotnet.configuration": e.currentTarget.value }).then(() => refreshSolution(true))}
        >
          <For each={ws()?.solutions.find((s) => s.path === ws()?.solution)?.configurations.map((c) => c.split("|")[0]).filter((c, i, all) => all.indexOf(c) === i) ?? ["Debug", "Release"]}>
            {(c) => <option value={c}>{c}</option>}
          </For>
        </select>
      </div>
      <div class="dn-split">
        <div class="dn-left">
          <Show when={dotnetWorkspaceError() && !ws()}>
            <div class="dn-message">
              <div>{t("langs.dotnet.cannotLoadTheSolution")}</div>
              <div class="muted">{dotnetWorkspaceError()}</div>
            </div>
          </Show>
          <Show when={!ws() && !dotnetWorkspaceError()}>
            <div class="dn-message muted">{dotnetLoading() ? t("langs.dotnet.loadingTheSolutionEllipsis") : t("langs.dotnet.noSolutionOrProject")}</div>
          </Show>
          <SolutionTree
            selected={selected()}
            onSelect={(k, p) => {
              setSelected(k);
              setProject(p);
            }}
          />
          <Show when={ws()?.problems?.length}>
            <div class="dn-message dn-warning">
              <For each={ws()!.problems!}>{(p) => <div>{p}</div>}</For>
            </div>
          </Show>
        </div>
        <BuildResults showOutput={showOutput()} />
      </div>
    </div>
  );
}
