/**
 * Hot Reload of `.NET Project` configurations that run with `dotnet watch`, like Rider's:
 * the run's output is read in Rust (`dotnet_run_output`) into Hot Reload events (enabled,
 * file changed, applied, rude edit, build failed), shown in the status bar. **Apply
 * Changes** (`UpdateRunningApplication`, Ctrl+F10 as IntelliJ's "Update Running
 * Application") saves the edited files so `dotnet watch` applies them, and answers its
 * restart question after a rude edit.
 */
import { createSignal, Show } from "solid-js";
import { Flame } from "lucide-solid";
import { dotnetWebApi, type HotReloadState, type WatchEvent } from "@/ipc/dotnetWeb";
import { runApi, type RunEvent } from "@/ipc/run";
import { registerStatusItem } from "@/features/statusItems";
import { getSetting } from "@/state/settings";
import { saveAll } from "@/state/editor";
import { dotnetActive, showDotnetBalloon } from "../state";
import { t } from "@/i18n";

export interface HotReloadStatus {
  runId: string;
  name: string;
  state: HotReloadState;
  message: string;
  at: number;
  prompt: boolean;
}

const [status, setStatus] = createSignal<HotReloadStatus | null>(null);
export { status as hotReloadStatus };

const LABELS: Record<HotReloadState, string> = {
  starting: "Starting",
  watching: "Watching",
  applying: "Applying changes…",
  applied: "Changes applied",
  noChanges: "No changes to apply",
  rudeEdit: "Restart needed",
  restarting: "Restarting",
  buildFailed: "Build failed",
  waiting: "Waiting for changes",
};

export const hotReloadLabel = (s: HotReloadState) => LABELS[s];

function apply(runId: string, name: string, events: WatchEvent[]) {
  for (const e of events) {
    setStatus({ runId, name, state: e.state, message: e.message, at: Date.now(), prompt: e.prompt });
    if (e.state === "buildFailed") showDotnetBalloon(`Hot Reload: ${name}`, e.message, 6000);
    if (e.prompt)
      showDotnetBalloon(`Hot Reload: ${name}`, "A rude edit needs a restart of the application.", 15000, [{ label: t("langs.dotnet.restart"), run: () => void runApi.write(runId, "y\n").catch(() => undefined) }]);
  }
}

/** Follow a `dotnet watch` run's output (called from the `.NET Project` type's start). */
export async function trackWatch(runId: string, name: string): Promise<void> {
  setStatus({ runId, name, state: "starting", message: "", at: Date.now(), prompt: false });
  let pending = "";
  let timer: ReturnType<typeof setTimeout> | undefined;
  const flush = () => {
    timer = undefined;
    const text = pending;
    pending = "";
    // Only complete lines; the rest waits for the next chunk.
    const cut = text.lastIndexOf("\n");
    if (cut < 0) return void (pending = text);
    pending = text.slice(cut + 1);
    void dotnetWebApi
      .runOutput(text.slice(0, cut + 1))
      .then((r) => apply(runId, name, r.watch))
      .catch(() => undefined);
  };
  const off = await runApi.onEvent(runId, (e: RunEvent) => {
    if (e.type === "output") {
      pending += e.text;
      timer ??= setTimeout(flush, 120);
    } else if (e.type === "exited") {
      off();
      if (status()?.runId === runId) setStatus(null);
    }
  });
}

/** Apply Changes: save the edited files; `dotnet watch` applies them (or restarts). */
export async function applyChanges(): Promise<void> {
  const s = status();
  if (!s) {
    showDotnetBalloon("No Hot Reload session", "Run a .NET Project configuration with 'dotnet watch (hot reload)' to apply changes to the running application.", 8000);
    return;
  }
  if (s.prompt) {
    await runApi.write(s.runId, "y\n").catch(() => undefined);
    setStatus({ ...s, state: "restarting", prompt: false, at: Date.now() });
    return;
  }
  await saveAll("explicit");
  setStatus({ ...s, state: "applying", message: "", at: Date.now() });
}

function HotReloadItem() {
  const time = (at: number) => new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return (
    <Show when={status()}>
      {(s) => (
        <button
          class="status-item dn-hot-reload"
          classList={{ bad: s().state === "buildFailed" || s().state === "rudeEdit", ok: s().state === "applied" }}
          data-status="hot-reload"
          title={t("langs.dotnet.hotReloadApplyChangesCtrl", { name: s().name, value: LABELS[s().state], value2: s().message ? `\n${s().message}` : "" })}
          onClick={() => void applyChanges()}
        >
          <Flame size={13} />
          <span>
            {LABELS[s().state]} {s().state === "applied" ? time(s().at) : ""}
          </span>
        </button>
      )}
    </Show>
  );
}

registerStatusItem({
  id: "dotnet-hot-reload",
  align: "right",
  order: 64,
  component: HotReloadItem,
  when: () => dotnetActive() && !!status() && getSetting<boolean>("dotnet.hotReload.showStatus", true) !== false,
});
