/**
 * The Profiler tool window, laid out like GoLand's: the profile's name and sample type on
 * top, then Flame Graph, Call Tree and Top (method list) tabs. A pprof profile (a profiled
 * Go Test run, or a `.pprof` file opened with Open Profile) is decoded in Rust
 * (`pprof_open`, `goforge-pprof`); this view only draws it. Clicking a frame opens its
 * source when it is a project file.
 */
import { createSignal, For, Show } from "solid-js";
import { FolderOpen } from "lucide-solid";
import { goApi, type ProfileView as Profile } from "@/ipc/go";
import { openFileAt } from "@/editor/views";
import { setInputDialog } from "@/features/git/overlays";
import { showProfiler } from "@/features/profiler";
import { formatValue, hashColor, ProfileBody, ProfileToolbar, unitOf, type FrameStyle, type ProfileTab } from "@/features/profiler/callTree";
import { workspace } from "@/state/workspace";
import { showGoBalloon } from "./state";

interface Opened {
  path: string;
  title: string;
  view: Profile;
}

const [opened, setOpened] = createSignal<Opened | null>(null);
const [tab, setTab] = createSignal<ProfileTab>("flame");

/** Open a profile in the Profiler (an absolute path or a project file). */
export async function openProfile(path: string, title?: string, sampleIndex?: number): Promise<void> {
  try {
    const view = await goApi.pprofOpen(path, sampleIndex);
    setOpened({ path, title: title ?? path.replace(/\\/g, "/").split("/").pop() ?? path, view });
    showProfiler("go");
  } catch (e) {
    showGoBalloon("Cannot open the profile", String(e).replace(/^Error: /, ""), 8000);
  }
}

export function askForProfile(): void {
  setInputDialog({
    title: "Open Profile",
    label: "pprof file (absolute path or project file):",
    value: "cpu.pprof",
    okLabel: "Open",
    onOk: (value) => openProfile(value.trim()),
  });
}

/** `github.com/acme/ledger/internal/money.Amount.String` → `money.Amount.String`. */
export const shortName = (name: string) => name.replace(/^.*\//, "");

const isProjectFile = (file: string) => {
  const root = workspace()?.root ?? "";
  return !!file && (!file.startsWith("/") || (!!root && file.startsWith(`${root}/`))) && !file.includes("/go/src/");
};

function navigate(file: string, line: number) {
  if (!isProjectFile(file)) return;
  const root = workspace()?.root ?? "";
  void openFileAt(file.startsWith(`${root}/`) ? file.slice(root.length + 1) : file, line || 1);
}

/** A stable color per package, from the theme's graph colors. */
function colorOf(name: string): string {
  if (/^(runtime|internal\/|syscall)/.test(shortName(name)) || name.startsWith("runtime.")) return "var(--graph-5)";
  if (/^(fmt|strconv|strings|bytes|sort|sync|reflect|os|io|testing|encoding)\b/.test(name)) return "var(--graph-3)";
  return hashColor(name.replace(/\.[^./]*(\.[^./]*)?$/, ""));
}

const style: FrameStyle = { navigate, isOwn: isProjectFile, shortName, colorOf };

/** Header buttons of the Profiler while it shows pprof profiles. */
export function ProfilerActions() {
  return (
    <button class="icon-btn" title="Open Profile (pprof)…" onClick={askForProfile}>
      <FolderOpen size={14} />
    </button>
  );
}

export function ProfilerView() {
  return (
    <div class="prof-view">
      <Show
        when={opened()}
        fallback={
          <div class="prof-empty muted">
            <p>No profile is open.</p>
            <p>Run a Go test or benchmark with a profile (Go Test configuration › Profile), or open a pprof file.</p>
            <button class="btn-secondary" onClick={askForProfile}>
              Open Profile…
            </button>
          </div>
        }
      >
        {(o) => (
          <>
            <ProfileToolbar
              title={o().title}
              tab={tab()}
              onTab={setTab}
              summary={`Total ${formatValue(o().view.total, unitOf(o().view))} · ${o().view.samples} call stacks${o().view.durationNanos ? ` · ${formatValue(o().view.durationNanos, "nanoseconds")} recorded` : ""}`}
            >
              <select
                class="prof-type"
                value={String(o().view.sampleIndex)}
                onChange={(e) => void openProfile(o().path, o().title, Number(e.currentTarget.value))}
              >
                <For each={o().view.sampleTypes}>
                  {(t, i) => (
                    <option value={String(i())}>
                      {t.kind} ({t.unit})
                    </option>
                  )}
                </For>
              </select>
            </ProfileToolbar>
            <ProfileBody view={o().view} tab={tab()} style={style} />
          </>
        )}
      </Show>
    </div>
  );
}
