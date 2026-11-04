import { For, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { GitBranch, Lock } from "lucide-solid";
import { activeTab } from "@/state/editor";
import { branchLabel } from "@/state/vcs";
import { statusMessage, visibleStatusItems } from "@/features/statusItems";
import { languageLabel } from "@/editor/languages";
import { NavigationBar, navBarLocation } from "@/features/navBar";
import { EncodingStatus, IndentStatus, LineSeparatorStatus } from "./FileStatusItems";
import { t } from "@/i18n";

export function StatusBar() {
  const tab = activeTab;
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
        {tab() && !tab()!.view && (
          <>
            <button
              class="status-item"
              aria-label={
                (tab()!.cursor.carets ?? 1) > 1
                  ? t("statusbar.carets", { count: tab()!.cursor.carets ?? 1 })
                  : t("a11y.caret.position", { line: tab()!.cursor.line, col: tab()!.cursor.col })
              }
            >
              {(tab()!.cursor.carets ?? 1) > 1 ? t("statusbar.carets", { count: tab()!.cursor.carets ?? 1 }) : `${tab()!.cursor.line}:${tab()!.cursor.col}`}
            </button>
            <LineSeparatorStatus tab={tab()!} />
            <EncodingStatus tab={tab()!} />
            <IndentStatus tab={tab()!} />
            <button class="status-item">{languageLabel(tab()!.path, tab()!.content)}</button>
          </>
        )}
        <button class="status-item" aria-label={t("a11y.statusbar.branch", { branch: branchLabel() })}>
          <GitBranch size={12} aria-hidden="true" /> {branchLabel()}
        </button>
        <button class="status-item" title={t("statusbar.writable")} aria-label={t("statusbar.writable")}>
          <Lock size={12} />
        </button>
      </div>
    </footer>
  );
}
