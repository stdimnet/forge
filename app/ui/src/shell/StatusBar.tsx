import { For } from "solid-js";
import { Dynamic } from "solid-js/web";
import { ChevronRight, GitBranch, Lock } from "lucide-solid";
import { activeTab, fileSystemFor } from "@/state/editor";
import { workspace } from "@/state/workspace";
import { currentBranch } from "@/state/vcs";
import { visibleStatusItems } from "@/features/statusItems";
import { languageLabel } from "@/editor/languages";
import { editorViewFor } from "./editorViews";

export function StatusBar() {
  const tab = activeTab;
  return (
    <footer class="status-bar">
      <div class="status-left">
        <span class="status-item">{workspace()?.name}</span>
        <For each={(() => {
          const t = tab();
          if (!t) return [];
          const fs = fileSystemFor(t.path);
          if (fs) return fs.breadcrumbs?.(t.path) ?? [t.name];
          const view = editorViewFor(t);
          return view ? (view.breadcrumbs?.(t) ?? [t.name]) : t.path.split("/");
        })()}>
          {(p) => (
            <>
              <ChevronRight size={11} class="muted" />
              <span class="status-item">{p}</span>
            </>
          )}
        </For>
        <For each={visibleStatusItems("left")}>{(s) => <Dynamic component={s.component} />}</For>
      </div>
      <div class="status-right">
        <For each={visibleStatusItems("right")}>{(s) => <Dynamic component={s.component} />}</For>
        {tab() && !tab()!.view && (
          <>
            <button class="status-item">
              {tab()!.cursor.line}:{tab()!.cursor.col}
            </button>
            <button class="status-item">LF</button>
            <button class="status-item">UTF-8</button>
            <button class="status-item">4 spaces</button>
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
