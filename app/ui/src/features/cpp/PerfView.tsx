/**
 * "perf" in the shared Profiler tool window: the profile of the last "Profile with perf"
 * run (`perf record -g`, then `perf script` turned into the call tree in Rust by
 * `cforge_analyzers::perf`), as Flame Graph, Call Tree and Method List. Double-clicking a
 * frame of the project opens its source line.
 */
import { createSignal, Show } from "solid-js";
import type { PerfProfile } from "@/ipc/cppTools";
import { openFileAt } from "@/editor/views";
import { showProfiler } from "@/features/profiler";
import { formatValue, hashColor, ProfileBody, ProfileToolbar, unitOf, type FrameStyle, type ProfileTab } from "@/features/profiler/callTree";
import { workspace } from "@/state/workspace";
import { t } from "@/i18n";

const [profile, setProfile] = createSignal<PerfProfile | null>(null);
const [tab, setTab] = createSignal<ProfileTab>("flame");
/** Why perf cannot profile here (macOS, Windows), from `cpp_native_tools`. */
const [unsupported, setUnsupported] = createSignal<string | null>(null);
export { setUnsupported as setPerfUnsupported };

/** Show a perf profile in the Profiler. */
export function showPerfProfile(p: PerfProfile): void {
  setProfile(p);
  showProfiler("perf");
}

const root = () => (workspace()?.root ?? "").replace(/\\/g, "/");

const isOwn = (file: string) => {
  if (!file) return false;
  const r = root();
  return !file.startsWith("/") ? !file.startsWith("[") : !!r && file.startsWith(`${r}/`);
};

function navigate(file: string, line: number) {
  const r = root();
  void openFileAt(file.startsWith(`${r}/`) ? file.slice(r.length + 1) : file, line || 1);
}

/** `stats::median(std::vector<double, …>)` → `stats::median(…)`. */
const shortName = (name: string) => name.replace(/\((.{24,})\)( const)?$/, "(…)$2");

function colorOf(name: string): string {
  if (name.startsWith("[") || /^(__libc|_start|__GI_|_dl_|__printf|__vfprintf|printf|malloc|free)/.test(name)) return "var(--graph-5)";
  if (/^(std::|__gnu_cxx::|void std::)/.test(name)) return "var(--graph-3)";
  const ns = /^([A-Za-z_][\w]*)::/.exec(name)?.[1] ?? name.replace(/\(.*$/, "");
  return hashColor(ns);
}

const style: FrameStyle = { navigate, isOwn, shortName, colorOf };

export function PerfView() {
  return (
    <div class="prof-view">
      <Show
        when={profile()}
        fallback={
          <div class="prof-empty muted">
            <p>{t("langs.cpp.noPerfProfileYet")}</p>
            <Show when={unsupported()} fallback={<p>{t("langs.cpp.runACmakeApplicationGoogle")}</p>}>
              <p>{unsupported()}</p>
            </Show>
          </div>
        }
      >
        {(p) => (
          <>
            <ProfileToolbar
              title={`perf: ${p().title}`}
              tab={tab()}
              onTab={setTab}
              summary={`${p().view.sampleTypes[p().view.sampleIndex]?.kind ?? ""} · Total ${formatValue(p().view.total, unitOf(p().view))} · ${p().view.samples} samples${p().view.durationNanos ? ` · ${formatValue(p().view.durationNanos, "nanoseconds")} recorded` : ""}`}
            />
            <ProfileBody view={p().view} tab={tab()} style={style} />
          </>
        )}
      </Show>
    </div>
  );
}
