import { For, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { GitBranch, Lock } from "lucide-solid";
import { activeTab } from "@/state/editor";
import { branchLabel } from "@/state/vcs";
import { statusMessage, visibleStatusItems } from "@/features/statusItems";
import { fileTypeOf } from "@/editor/fileTypes";
import { openFileTypePopup } from "@/features/fileTypes/override";
import { NavigationBar, navBarLocation } from "@/features/navBar";
import { EncodingStatus, IndentStatus, LineSeparatorStatus } from "./FileStatusItems";
import { t } from "@/i18n";

export function StatusBar() {
  /** The active tab when it is a text editor (not an image or other view tab). */
  const textTab = () => {
    const tab = activeTab();
    return tab && !tab.view ? tab : undefined;
  };
  return (
    <footer class="status-bar" aria-label={t("a11y.statusbar")}>
      <div class="status-left">
        <Show when={navBarLocation() !== "hidden"}>
          <NavigationBar placement="statusBar" />
        </Show>
        <For each={visibleStatusItems("left")}>{(s) => <Dynamic component={s.component} />}</For>
        <Show when={statusMessage()}>
          {(text) => (
            <span class="status-message" title={text()} role="status">
              {text()}
            </span>
          )}
        </Show>
      </div>
      <div class="status-right">
        <For each={visibleStatusItems("right")}>{(s) => <Dynamic component={s.component} />}</For>
        {/* Keyed <Show>: the inner items must never read a tab that is no longer active (#220). */}
        <Show when={textTab()} keyed>
          {(active) => (
            <>
              <button
                class="status-item"
                data-status="caret"
                aria-label={
                  (active.cursor.carets ?? 1) > 1
                    ? t("statusbar.carets", { count: active.cursor.carets ?? 1 })
                    : t("a11y.caret.position", { line: active.cursor.line, col: active.cursor.col })
                }
              >
                {(active.cursor.carets ?? 1) > 1 ? t("statusbar.carets", { count: active.cursor.carets ?? 1 }) : `${active.cursor.line}:${active.cursor.col}`}
              </button>
              <LineSeparatorStatus tab={active} />
              <EncodingStatus tab={active} />
              <IndentStatus tab={active} />
              <FileTypeStatus path={active.path} content={active.content} />
            </>
          )}
        </Show>
        <Show when={branchLabel()}>
          <button class="status-item" aria-label={t("a11y.statusbar.branch", { branch: branchLabel() })}>
            <GitBranch size={12} aria-hidden="true" /> {branchLabel()}
          </button>
        </Show>
        <button class="status-item" title={t("statusbar.writable")} aria-label={t("statusbar.writable")}>
          <Lock size={12} />
        </button>
      </div>
    </footer>
  );
}

/** The effective file type; click to override it (File | File Properties | Override File Type). */
function FileTypeStatus(props: { path: string; content: string }) {
  const info = () => fileTypeOf(props.path, props.content);
  const tooltip = () =>
    info().overridden ? t("statusbar.fileType.overridden", { type: info().label }) : t("statusbar.fileType.tooltip", { type: info().label });
  return (
    <button
      class="status-item"
      data-status="file-type"
      classList={{ "status-overridden": !!info().overridden }}
      title={tooltip()}
      aria-label={tooltip()}
      onClick={() => void openFileTypePopup(props.path)}
    >
      {info().label}
    </button>
  );
}
