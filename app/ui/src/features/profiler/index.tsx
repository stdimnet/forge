/**
 * The Profiler tool window shared by the language packs: each pack registers a profiler
 * source (PyForge: cProfile snapshots, GoForge: pprof profiles) with its own view and header
 * buttons; the window shows the active sources (tabs when a project uses several) and opens
 * on the one that showed a profile last (`showProfiler`).
 */
import { createMemo, createSignal, For, Show, type Component } from "solid-js";
import { Dynamic } from "solid-js/web";
import { Gauge } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerKeymapActions } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { showToolWindow, toggleToolWindow } from "@/state/layout";
import "./profiler-host.css";
import { t } from "@/i18n";

export interface ProfilerSource {
  id: string;
  /** Tab label when several sources are active ("cProfile", "pprof"). */
  label: string;
  /** Whether the source applies to the open project. */
  active: () => boolean;
  component: Component;
  /** Header buttons of the tool window while the source is shown. */
  actions?: Component;
  /** Called when the tool window is opened with the action (load the newest profile). */
  onActivate?: () => void;
  order?: number;
}

const [sources, setSources] = createSignal<ProfilerSource[]>([]);
const [selected, setSelected] = createSignal<string | null>(null);

export function registerProfilerSource(s: ProfilerSource): void {
  setSources((prev) => [...prev.filter((x) => x.id !== s.id), s]);
}

const active = createMemo(() => sources().filter((s) => s.active()).sort((a, b) => (a.order ?? 0) - (b.order ?? 0)));
const current = () => active().find((s) => s.id === selected()) ?? active()[0];

/** Show the Profiler with a source's view (after that source opened a profile). */
export function showProfiler(id: string): void {
  setSelected(id);
  showToolWindow("profiler");
}

function ProfilerWindow() {
  return (
    <div class="profiler-host">
      <Show when={active().length > 1}>
        <div class="profiler-host-tabs">
          <For each={active()}>
            {(s) => (
              <button classList={{ active: current()?.id === s.id }} onClick={() => setSelected(s.id)}>
                {s.label}
              </button>
            )}
          </For>
        </div>
      </Show>
      <div class="profiler-host-body">
        <Show when={current()}>{(s) => <Dynamic component={s().component} />}</Show>
      </div>
    </div>
  );
}

function ProfilerActions() {
  return <Show when={current()?.actions}>{(a) => <Dynamic component={a()} />}</Show>;
}

registerToolWindow({
  id: "profiler",
  title: t("toolwindow.profiler.title"),
  icon: Gauge,
  anchor: "bottom",
  order: 47,
  component: ProfilerWindow,
  actions: ProfilerActions,
  when: () => active().length > 0,
});

registerKeymapActions([{ id: "ActivateProfilerToolWindow", title: "Profiler", group: "Tool Windows" }]);

registerCommand({
  id: "ActivateProfilerToolWindow",
  title: "Profiler",
  category: "View",
  run: () => {
    toggleToolWindow("profiler");
    current()?.onActivate?.();
  },
});
