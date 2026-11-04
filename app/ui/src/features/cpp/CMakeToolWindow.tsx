/**
 * The CMake tool window, like CLion's: one tab per enabled CMake profile (the active one
 * selected), the toolbar (Reload CMake Project, Reset Cache and Reload Project, Stop, Clean,
 * CMake Settings), the targets of the profile's code model (cmake-file-api: executables,
 * libraries, utilities; build, run, debug and Go to Definition in CMakeLists.txt) and the
 * configure log, where `CMake Error at CMakeLists.txt:6` lines open the file at that line.
 */
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import {
  AppWindow,
  Bug,
  CircleAlert,
  Eraser,
  Hammer,
  Library,
  LoaderCircle,
  Play,
  RefreshCw,
  RotateCcw,
  Settings,
  Square,
  SquareFunction,
  TriangleAlert,
  Wrench,
} from "lucide-solid";
import type { CMakeTarget, ConfigureMessage } from "@/ipc/cpp";
import { cppApi } from "@/ipc/cpp";
import { useTemporary } from "@/features/run/configStore";
import { debugConfiguration, launch } from "@/features/run/runStore";
import { openSettings } from "@/features/settings/dialogState";
import { openFileAt } from "@/editor/views";
import { saveSetting } from "@/state/settings";
import { cmakeLog, cmakeProfiles, cmakeState, cmakeTargets, refreshCMake, refreshProfiles, reloadCMake, runBuild } from "./state";
import { t as tr } from "@/i18n";
import "./cpp.css";

const KIND_LABEL: Record<CMakeTarget["kind"], string> = {
  executable: "Executable",
  staticLibrary: "Static library",
  sharedLibrary: "Shared library",
  moduleLibrary: "Module library",
  objectLibrary: "Object library",
  interfaceLibrary: "Interface library",
  utility: "Utility",
};

const kindIcon = (t: CMakeTarget) => (t.kind === "executable" ? AppWindow : t.kind === "utility" ? SquareFunction : Library);

const [selected, setSelected] = createSignal<string | null>(null);

function appConfiguration(t: CMakeTarget) {
  return useTemporary({ name: t.name, type: "cmake-app", options: { target: t.name } });
}

function jumpTo(t: CMakeTarget): void {
  const at = t.definedAt;
  if (at) void openFileAt(at.file, at.line);
}

function TargetRow(props: { t: CMakeTarget }) {
  const t = () => props.t;
  const exe = () => t().kind === "executable";
  return (
    <div
      class="tree-row cmake-target"
      classList={{ selected: selected() === t().name }}
      data-testid="cmake-target"
      data-target={t().name}
      tabIndex={0}
      title={`${KIND_LABEL[t().kind]}${t().artifacts?.[0] ? `: ${t().artifacts![0]}` : ""}`}
      onClick={() => setSelected(t().name)}
      onDblClick={() => jumpTo(t())}
      onKeyDown={(e) => e.key === "Enter" && jumpTo(t())}
    >
      <Dynamic component={kindIcon(t())} size={14} class={`cmake-target-icon cmake-${t().kind}`} />
      <span class="tree-label">{t().name}</span>
      <span class="tree-hint">{KIND_LABEL[t().kind]}</span>
      <span class="cmake-row-actions">
        <button class="icon-btn" title={tr("langs.cpp.build", { name: t().name })} onClick={(e) => (e.stopPropagation(), void runBuild("build", [t().name]))}>
          <Hammer size={13} />
        </button>
        <Show when={exe()}>
          <button class="icon-btn" title={tr("common.run", { name: t().name })} onClick={(e) => (e.stopPropagation(), void launch(appConfiguration(t())))}>
            <Play size={13} />
          </button>
          <button class="icon-btn" title={tr("common.debug", { name: t().name })} onClick={(e) => (e.stopPropagation(), void debugConfiguration(appConfiguration(t())))}>
            <Bug size={13} />
          </button>
        </Show>
      </span>
    </div>
  );
}

function Targets() {
  const groups = createMemo(() => {
    const list = cmakeTargets();
    return [
      { title: tr("langs.cpp.executables"), items: list.filter((t) => t.kind === "executable") },
      { title: tr("langs.cpp.libraries"), items: list.filter((t) => t.kind.endsWith("Library")) },
      { title: tr("langs.cpp.utilities"), items: list.filter((t) => t.kind === "utility") },
    ].filter((g) => g.items.length);
  });
  const project = () => cmakeState()?.model?.configurations[0]?.projects[0] ?? "";
  return (
    <div class="cmake-targets" data-testid="cmake-targets">
      <Show
        when={groups().length}
        fallback={
          <div class="muted cmake-pad">
            {cmakeState()?.status === "noCMake" ? tr("langs.cpp.theProjectHasNoCmakelists") : cmakeState()?.status === "loading" ? tr("langs.cpp.loadingTheCmakeProjectEllipsis") : tr("langs.cpp.noTargetsReloadTheCmake")}
          </div>
        }
      >
        <div class="tree-row cmake-project">
          <Wrench size={14} class="cmake-project-icon" />
          <span class="tree-label">{project()}</span>
          <span class="tree-hint">{cmakeState()?.model?.generator}</span>
        </div>
        <For each={groups()}>
          {(g) => (
            <>
              <div class="cmake-group">{g.title}</div>
              <For each={g.items}>{(t) => <TargetRow t={t} />}</For>
            </>
          )}
        </For>
      </Show>
    </div>
  );
}

/** The message whose header is on log line `i`. */
function messageAt(messages: ConfigureMessage[], i: number): ConfigureMessage | undefined {
  return messages.find((m) => m.logLine === i);
}

function Log() {
  let el: HTMLDivElement | undefined;
  const lines = () => cmakeLog();
  const messages = () => cmakeState()?.messages ?? [];
  // Lines of a message's explanation (indented, after its header).
  const tone = createMemo(() => {
    const out = new Map<number, string>();
    const all = lines();
    for (const m of messages()) {
      out.set(m.logLine, m.severity);
      for (let i = m.logLine + 1; i < all.length && (all[i].line.startsWith(" ") || !all[i].line.trim()); i++) out.set(i, `${m.severity} body`);
    }
    return out;
  });
  // Show the first error or warning of the run, else the end of the log.
  const scroll = () =>
    queueMicrotask(() => {
      if (!el) return;
      const first = el.querySelector<HTMLElement>(".cmake-log-line.error, .cmake-log-line.warning, .cmake-log-line.deprecation");
      el.scrollTop = first ? Math.max(0, first.offsetTop - el.offsetTop - 24) : el.scrollHeight;
    });
  onMount(scroll);
  return (
    <div class="cmake-log" ref={el} data-testid="cmake-log">
      <For each={lines()}>
        {(l, i) => {
          const m = () => messageAt(messages(), i());
          return (
            <div class={`cmake-log-line ${tone().get(i()) ?? ""}`} classList={{ "cmake-stderr": l.stream === "stderr", "cmake-cmd": l.stream === "system" }}>
              <Show when={m()?.file} fallback={l.line || " "}>
                <span>{l.line.slice(0, l.line.indexOf(m()!.file!))}</span>
                <a
                  class="cmake-link"
                  href="#"
                  title={tr("langs.cpp.open", { file: m()!.file, value: m()!.line ?? 1 })}
                  onClick={(e) => {
                    e.preventDefault();
                    void openFileAt(m()!.file!, m()!.line ?? 1);
                  }}
                >
                  {l.line.slice(l.line.indexOf(m()!.file!), l.line.indexOf(m()!.file!) + m()!.file!.length + (m()!.line ? String(m()!.line).length + 1 : 0))}
                </a>
                <span>{l.line.slice(l.line.indexOf(m()!.file!) + m()!.file!.length + (m()!.line ? String(m()!.line).length + 1 : 0))}</span>
              </Show>
            </div>
          );
        }}
      </For>
    </div>
  );
}

function Summary() {
  const s = () => cmakeState();
  const errors = () => (s()?.messages ?? []).filter((m) => m.severity === "error").length;
  const warnings = () => (s()?.messages ?? []).filter((m) => m.severity !== "error").length;
  return (
    <div class="cmake-summary" data-testid="cmake-summary">
      <Show when={s()?.status === "loading"}>
        <LoaderCircle size={13} class="lsp-spin" /> {tr("langs.cpp.loadingCmakeProjectEllipsis")}
      </Show>
      <Show when={s()?.status === "loaded"}>
        <span class="cmake-ok">{tr("langs.cpp.finished")}</span> {s()?.model?.cmakeVersion ? `CMake ${s()!.model!.cmakeVersion}` : ""}
      </Show>
      <Show when={s()?.status === "failed"}>
        <span class="cmake-failed">{tr("langs.cpp.failedToReload")}</span>
      </Show>
      <Show when={errors()}>
        <span class="cmake-count">
          <CircleAlert size={12} class="sev-error" /> {errors()}
        </span>
      </Show>
      <Show when={warnings()}>
        <span class="cmake-count">
          <TriangleAlert size={12} class="sev-warning" /> {warnings()}
        </span>
      </Show>
      <span class="tree-hint cmake-build-dir" title={s()?.buildDir}>
        {s()?.buildDir}
      </span>
    </div>
  );
}

export function CMakeToolWindow() {
  onMount(() => {
    void refreshProfiles();
    if (!cmakeState()) void refreshCMake();
  });
  const enabled = () => (cmakeProfiles()?.profiles ?? []).filter((p) => p.enabled);
  const active = () => cmakeState()?.profile ?? cmakeProfiles()?.active;
  const select = async (name: string) => {
    if (name === active()) return;
    await saveSetting("cmake.activeProfile", name);
    await refreshProfiles();
    const s = await refreshCMake();
    if (s && s.status === "notLoaded") void reloadCMake();
  };
  const loading = () => cmakeState()?.status === "loading";
  return (
    <div class="cmake-tw" data-testid="cmake-tool-window">
      <div class="cmake-side-toolbar">
        <button class="icon-btn" title={tr("langs.cpp.reloadCmakeProject")} disabled={loading()} data-testid="cmake-reload" onClick={() => void reloadCMake()}>
          <RefreshCw size={14} />
        </button>
        <button class="icon-btn" title={tr("langs.cpp.resetCacheAndReloadProject")} disabled={loading()} onClick={() => void reloadCMake(true)}>
          <RotateCcw size={14} />
        </button>
        <button class="icon-btn" title={tr("common.stop")} disabled={!loading()} onClick={() => void cppApi.cancel()}>
          <Square size={13} />
        </button>
        <div class="cmake-sep" />
        <button class="icon-btn" title={tr("langs.cpp.clean")} onClick={() => void runBuild("clean")}>
          <Eraser size={14} />
        </button>
        <button class="icon-btn" title={tr("langs.cpp.cmakeSettings")} onClick={() => openSettings("build.cmake")}>
          <Settings size={14} />
        </button>
      </div>
      <div class="cmake-main">
        <div class="cmake-tabs" role="tablist">
          <For each={enabled()}>
            {(p) => (
              <button class="cmake-tab" role="tab" classList={{ active: p.name === active() }} onClick={() => void select(p.name)} title={p.preset ? `Preset ${p.preset}` : `${p.buildType} profile`}>
                {p.name}
              </button>
            )}
          </For>
        </div>
        <Summary />
        <div class="cmake-split">
          <Targets />
          <Log />
        </div>
      </div>
    </div>
  );
}
