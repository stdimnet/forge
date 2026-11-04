/**
 * File Watchers (Settings | Tools | File Watchers): the settings page, runs after saves,
 * auto-save while typing, external changes, and the "File Watchers" tab of the Problems
 * tool window with what the watchers' output filters found.
 */
import { For, Show } from "solid-js";
import { EditorView } from "@codemirror/view";
import { CircleAlert, Eye } from "lucide-solid";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerProblemsTab } from "@/features/lsp/problemsTabs";
import { registerEditorExtension } from "@/editor/extensions";
import { openFileAt } from "@/editor/views";
import { onFileSaved, saveFile } from "@/state/editor";
import { onFsChanged } from "@/state/fsWatch";
import { FileWatchersPage } from "./FileWatchersPage";
import { autoSaveWanted, clearWatcherProblems, fileWatchers, triggerWatchers, watcherProblemCount, watcherProblems } from "./run";
import { t } from "@/i18n";

export { triggerWatchers } from "./run";

registerSettingsPage({
  id: "tools.file-watchers",
  title: t("settings.page.tools.file-watchers"),
  group: "Tools",
  order: 31,
  keywords: ["file watchers", "sass", "scss", "less", "tsc", "typescript", "babel", "prettier", "track only root files", "$FileName$", "output filters"],
  component: FileWatchersPage,
});

function WatcherProblemsView() {
  return (
    <div class="fw-problems">
      <div class="fw-problems-toolbar">
        <button class="btn-secondary" onClick={clearWatcherProblems} disabled={!watcherProblemCount()}>
          {t("common.clear")}
        </button>
      </div>
      <Show when={watcherProblems().length} fallback={<div class="set-list-empty">{t("langs.webTooling.noProblemsReportedByFile")}</div>}>
        <For each={watcherProblems()}>
          {(set) => (
            <div class="fw-problem-group">
              <div class="fw-problem-head">
                <Eye size={13} class="muted" /> {set.watcher} <span class="muted">{set.file}</span>
              </div>
              <For each={set.problems}>
                {(p) => (
                  <div class="fw-problem" onClick={() => void openFileAt(p.path, p.line ?? 1, p.column ?? 1)}>
                    <CircleAlert size={13} class="fw-error-icon" />
                    <span class="fw-problem-msg">{p.message}</span>
                    <span class="muted">
                      {p.path}
                      {p.line ? `:${p.line}` : ""}
                      {p.column ? `:${p.column}` : ""}
                    </span>
                  </div>
                )}
              </For>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}

registerProblemsTab({
  id: "file-watchers",
  title: "File Watchers",
  order: 30,
  count: watcherProblemCount,
  visible: () => fileWatchers().length > 0 || watcherProblemCount() > 0,
  component: WatcherProblemsView,
});

const isProjectFile = (path: string) => !path.includes("://") && !path.startsWith("forge:");

onFileSaved((path) => {
  if (isProjectFile(path)) void triggerWatchers([path], "save");
});

onFsChanged((c) => {
  if (c.changed.length && fileWatchers().some((w) => w.enabled && w.onExternal)) void triggerWatchers(c.changed, "external");
});

// "Auto-save edited files to trigger the watcher": save after a pause in typing.
const AUTO_SAVE_DELAY = 750;
registerEditorExtension(({ path }) => {
  if (!isProjectFile(path)) return null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  return EditorView.updateListener.of((u) => {
    if (!u.docChanged || !fileWatchers().some((w) => w.enabled && w.autoSave)) return;
    clearTimeout(timer);
    timer = setTimeout(async () => {
      if (await autoSaveWanted(path)) await saveFile(path).catch(() => undefined);
    }, AUTO_SAVE_DELAY);
  });
});
