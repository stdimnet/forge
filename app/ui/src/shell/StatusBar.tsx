import { For, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { GitBranch, Lock } from "lucide-solid";
import { activeTab } from "@/state/editor";
import { currentBranch } from "@/state/vcs";
import { visibleStatusItems } from "@/features/statusItems";
import { languageLabel } from "@/editor/languages";
import { NavigationBar, navBarLocation } from "@/features/navBar";
import { EncodingStatus, IndentStatus, LineSeparatorStatus } from "./FileStatusItems";

export function StatusBar() {
  const tab = activeTab;
  return (
    <footer class="status-bar">
      <div class="status-left">
        <Show when={navBarLocation() !== "hidden"}>
          <NavigationBar placement="statusBar" />
        </Show>
        <For each={visibleStatusItems("left")}>{(s) => <Dynamic component={s.component} />}</For>
      </div>
      <div class="status-right">
        <For each={visibleStatusItems("right")}>{(s) => <Dynamic component={s.component} />}</For>
        {tab() && !tab()!.view && (
          <>
            <button class="status-item">
              {(tab()!.cursor.carets ?? 1) > 1 ? `${tab()!.cursor.carets} carets` : `${tab()!.cursor.line}:${tab()!.cursor.col}`}
            </button>
            <LineSeparatorStatus tab={tab()!} />
            <EncodingStatus tab={tab()!} />
            <IndentStatus tab={tab()!} />
            <button class="status-item">{languageLabel(tab()!.path, tab()!.content)}</button>
          </>
        )}
        <button class="status-item">
          <GitBranch size={12} /> {currentBranch()}
        </button>
        <button class="status-item" title="File is writable">
          <Lock size={12} />
        </button>
      </div>
    </footer>
  );
}
