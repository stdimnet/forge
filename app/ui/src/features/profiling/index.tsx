/**
 * Profiling for PHP and Node.js (PHPStorm's Xdebug and SPX snapshot views, the V8 CPU and
 * memory profilers): four sources of the shared Profiler tool window and their actions.
 *
 * - Xdebug and SPX (PHP projects): Run | Profile with Xdebug / Profile with SPX run the
 *   selected PHP configuration with `RunRequest.profiler`; Tools | Analyze Xdebug Profiler
 *   Snapshot… opens a `cachegrind.out.*` file.
 * - V8 CPU and V8 Heap (JavaScript projects): Run | Profile with V8 / Profile Allocations
 *   with V8 run a Node.js or npm configuration with `--cpu-prof` / `--heap-prof`; heap
 *   snapshots (`.heapsnapshot`) open in the Memory view.
 * - Tools | Open Profiler Snapshot… opens any of these files. When a profiling run ends,
 *   Rust emits its snapshot (`{ type: "profiler" }`) and the Profiler shows it.
 */
import { FileInput, History } from "lucide-solid";
import { isTauri } from "@/ipc";
import { profilerApi, type RunProfiler } from "@/ipc/profiler";
import { registerProfilerSource } from "@/features/profiler";
import { action, registerMainMenuEntries } from "@/features/mainMenu/model";
import { openMenu } from "@/features/run/PopupMenu";
import { selectedConfiguration } from "@/features/run/configStore";
import { launch, onRunEvent } from "@/features/run/runStore";
import { openRunConfigurations } from "@/features/run/dialogState";
import { registerKeymapActions } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { isPluginActive } from "@/state/project";
import { workspace } from "@/state/workspace";
import { SnapshotView } from "./SnapshotView";
import { loadedSnapshot, openSnapshot, showBalloon, showSnapshot, SOURCE_LABELS, type SourceId } from "./store";
import { t } from "@/i18n";

const phpActive = () => isPluginActive("forge.php");
const jsActive = () => isPluginActive("forge.web");

const PHP_TYPES = new Set(["php-script", "phpunit", "pest", "codeception", "behat", "phpspec", "artisan", "php-builtin-server"]);
const NODE_TYPES = new Set(["node", "npm"]);

const FILTERS = {
  xdebug: { name: "Xdebug profiler snapshots", extensions: ["*", "gz"] },
  all: { name: "Profiler snapshots", extensions: ["*", "gz", "json", "cpuprofile", "heapprofile", "heapsnapshot"] },
};

const when = (at: number) => new Date(at).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

/** Earlier profiling runs' snapshots (in browser mode: the demo snapshots). */
async function recentMenu(x: number, y: number): Promise<void> {
  const list = await profilerApi.recent().catch(() => []);
  openMenu({
    x,
    y,
    items: list.length ? list.map((s) => ({ label: s.name, shortcut: when(s.at), run: () => void openSnapshot(s.path) })) : [{ label: t("tools.profiling.noSnapshotsYet"), disabled: true }],
  });
}

/** Pick a snapshot file and open it. */
async function pickSnapshot(filter: keyof typeof FILTERS): Promise<void> {
  if (!isTauri()) {
    const r = document.querySelector<HTMLElement>('[data-testid="profiler-actions"]')?.getBoundingClientRect();
    return recentMenu(r?.left ?? window.innerWidth / 2 - 120, (r?.bottom ?? window.innerHeight / 3) + 2);
  }
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ multiple: false, defaultPath: workspace()?.root, filters: [FILTERS[filter]] });
  if (typeof picked === "string") await openSnapshot(picked);
}

function Actions() {
  return (
    <span class="profiling-actions" data-testid="profiler-actions">
      <button
        class="icon-btn"
        title={t("tools.profiling.recentSnapshotsEllipsis")}
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          void recentMenu(r.left, r.bottom + 2);
        }}
      >
        <History size={14} />
      </button>
      <button class="icon-btn" title={t("tools.profiling.openProfilerSnapshotEllipsis")} onClick={() => void pickSnapshot("all")}>
        <FileInput size={14} />
      </button>
    </span>
  );
}

const SOURCES: { id: SourceId; active: () => boolean; order: number }[] = [
  { id: "xdebug", active: phpActive, order: 40 },
  { id: "spx", active: phpActive, order: 41 },
  { id: "v8-cpu", active: jsActive, order: 42 },
  { id: "v8-heap", active: jsActive, order: 43 },
];

for (const s of SOURCES) {
  registerProfilerSource({
    id: s.id,
    label: SOURCE_LABELS[s.id],
    active: () => s.active() || !!loadedSnapshot(s.id),
    component: () => <SnapshotView source={s.id} />,
    actions: Actions,
    order: s.order,
  });
}

onRunEvent((_session, e) => {
  if (e.type === "profiler" && e.snapshot) showSnapshot(e.snapshot, e.path ?? "");
});

// ---- Run | Profile with … ----------------------------------------------------------------

const RUN_LABEL: Record<RunProfiler, string> = {
  xdebug: "Xdebug",
  spx: "SPX",
  "v8-cpu": "V8",
  "v8-heap": "V8 (allocations)",
};

async function profileWith(kind: RunProfiler): Promise<void> {
  const cfg = selectedConfiguration();
  if (!cfg) return openRunConfigurations();
  const php = kind === "xdebug" || kind === "spx";
  if (!(php ? PHP_TYPES : NODE_TYPES).has(cfg.type)) {
    showBalloon(
      t("tools.profiling.cannotProfileWith", { name: cfg.name, value: RUN_LABEL[kind] }),
      php ? t("tools.profiling.chooseAPhpScriptPhpunit") : t("tools.profiling.chooseANodeJsOr"),
      6000,
    );
    return;
  }
  await launch(cfg, { extra: { profiler: kind } });
}

registerKeymapActions([
  { id: "profiling.profileXdebug", title: "Profile with Xdebug", group: "Main Menu/Run" },
  { id: "profiling.profileSpx", title: "Profile with SPX", group: "Main Menu/Run" },
  { id: "profiling.profileV8", title: "Profile with V8", group: "Main Menu/Run" },
  { id: "profiling.profileV8Heap", title: "Profile Allocations with V8", group: "Main Menu/Run" },
  { id: "profiling.analyzeXdebugSnapshot", title: "Analyze Xdebug Profiler Snapshot…", group: "Main Menu/Tools" },
  { id: "profiling.openSnapshot", title: "Open Profiler Snapshot…", group: "Main Menu/Tools" },
]);

registerCommand({ id: "profiling.profileXdebug", title: "Profile with Xdebug", category: "Run", run: () => void profileWith("xdebug") });
registerCommand({ id: "profiling.profileSpx", title: "Profile with SPX", category: "Run", run: () => void profileWith("spx") });
registerCommand({ id: "profiling.profileV8", title: "Profile with V8", category: "Run", run: () => void profileWith("v8-cpu") });
registerCommand({ id: "profiling.profileV8Heap", title: "Profile Allocations with V8", category: "Run", run: () => void profileWith("v8-heap") });
registerCommand({
  id: "profiling.analyzeXdebugSnapshot",
  title: "Analyze Xdebug Profiler Snapshot…",
  category: "Tools",
  run: () => void pickSnapshot("xdebug"),
});
registerCommand({ id: "profiling.openSnapshot", title: "Open Profiler Snapshot…", category: "Tools", run: () => void pickSnapshot("all") });

registerMainMenuEntries("Run", [action("profiling.profileXdebug", "Profile with _Xdebug"), action("profiling.profileSpx", "Profile with _SPX")], {
  after: "Debug",
  when: phpActive,
});
registerMainMenuEntries("Run", [action("profiling.profileV8", "Profile with _V8"), action("profiling.profileV8Heap", "Profile _Allocations with V8")], {
  after: "Debug",
  when: jsActive,
});
registerMainMenuEntries("Tools", [action("profiling.analyzeXdebugSnapshot", "Analyze _Xdebug Profiler Snapshot...")], { when: phpActive });
registerMainMenuEntries("Tools", [action("profiling.openSnapshot", "Open Profiler Snapshot...")], { when: () => phpActive() || jsActive() });
