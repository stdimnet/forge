/**
 * Select In… (Alt+F1, PHPStorm's `SelectIn`): a popup that shows the current file in another
 * view: Project View, Commit, Structure, File Structure, Navigation Bar, Git Log, the system
 * file manager (Explorer / Finder / File Manager) and Terminal. Targets are numbered 1–9 like
 * PHPStorm and picked with the digit, the arrows and Enter, or a click. The file is the
 * active editor's, or the file selected in the Project view when it has the focus.
 */
import { For, Show, createSignal, onMount } from "solid-js";
import { render } from "solid-js/web";
import { registerCommand } from "@/state/commands";
import { editor, isCustomTab } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { statusOf } from "@/state/vcs";
import { actionById, runAction, showBalloon } from "@/keymap";
import { windowApi } from "@/ipc/window";
import { chrome } from "@/shell/window/chrome";
import { activeView } from "@/features/navigation/editorCommands";
import { projectTree, selectInTree } from "@/features/project/ProjectView";
import { hasFileStructure, selectStructureAtCaret } from "@/features/php/StructureView";
import { selectCommitFile } from "@/features/git/CommitPanel";
import { setLogPathFilter } from "@/features/git/GitLogPanel";
import { repoAvailable } from "@/features/git/store";
import { openTerminalAt } from "@/features/terminal";
import { t as tr } from "@/i18n";
import "./selectIn.css";

export interface SelectInTarget {
  id: string;
  label: string;
  run: () => void | Promise<void>;
}

/** What Select In… acts on. */
export interface SelectInContext {
  path: string;
  /** Opened from the editor (with a caret), not from the Project view. */
  fromEditor: boolean;
}

/** The file manager's name on this OS, like PHPStorm's menus. */
export function fileManagerName(os = chrome()?.os): string {
  return os === "windows" ? "Explorer" : os === "macos" ? "Finder" : "File Manager";
}

const dirOf = (path: string) => path.split("/").slice(0, -1).join("/");

/** The targets that apply to `ctx`, in PHPStorm's order. */
export function selectInTargets(ctx: SelectInContext): SelectInTarget[] {
  const { path, fromEditor } = ctx;
  const out: SelectInTarget[] = [];
  // From the Project view the file is already selected there.
  if (fromEditor)
    out.push({
      id: "project",
      label: tr("tools.editorNavigation.projectView"),
      run: () => {
        showToolWindow("project");
        selectInTree(path);
      },
    });
  if (statusOf(path))
    out.push({
      id: "commit",
      label: tr("tools.editorNavigation.commit"),
      run: () => {
        showToolWindow("commit");
        selectCommitFile(path);
      },
    });
  if (hasFileStructure(path) && fromEditor) {
    out.push({
      id: "structure",
      label: tr("tools.editorNavigation.structure"),
      run: () => {
        showToolWindow("structure");
        // The view loads the file's structure first.
        setTimeout(selectStructureAtCaret, 250);
      },
    });
  }
  if (fromEditor) {
    out.push({ id: "fileStructure", label: tr("tools.editorNavigation.fileStructure"), run: () => runAction("FileStructurePopup") });
    out.push({ id: "navBar", label: tr("tools.editorNavigation.navigationBar"), run: () => runAction("ShowNavBar") });
  }
  if (repoAvailable())
    out.push({
      id: "gitLog",
      label: tr("tools.editorNavigation.gitLog"),
      run: () => {
        setLogPathFilter(path);
        showToolWindow("git");
      },
    });
  out.push({
    id: "fileManager",
    label: fileManagerName(),
    run: () =>
      windowApi.revealInFileManager(path).catch((e) => {
        showBalloon(tr("tools.editorNavigation.cannotShowTheFileIn", { fileManagerName: fileManagerName() }), String(e instanceof Error ? e.message : e));
      }),
  });
  out.push({ id: "terminal", label: tr("tools.editorNavigation.terminal"), run: () => openTerminalAt(dirOf(path)) });
  return out;
}

const [session, setSession] = createSignal<{ ctx: SelectInContext; targets: SelectInTarget[] } | null>(null);
export { session as selectInSession };

/** The file Select In… acts on: the Project view's selection when it has the focus. */
function currentContext(): SelectInContext | null {
  const inTree = document.activeElement?.closest(".project-tree");
  const selected = projectTree.selected();
  if (inTree && selected && !selected.includes("://")) return { path: selected, fromEditor: false };
  const path = editor.active;
  if (!path || isCustomTab(path) || path.includes("://")) return null;
  return { path, fromEditor: true };
}

export function openSelectIn(ctx = currentContext()): void {
  if (!ctx) return;
  setSession({ ctx, targets: selectInTargets(ctx) });
}

function close(refocus = true): void {
  setSession(null);
  if (refocus) activeView()?.focus();
}

function SelectInPopup(props: { targets: SelectInTarget[]; path: string }) {
  let root!: HTMLDivElement;
  const [index, setIndex] = createSignal(0);
  onMount(() => root.focus());

  const pick = (t: SelectInTarget | undefined) => {
    if (!t) return;
    close(t.id === "fileStructure" || t.id === "navBar");
    void t.run();
  };

  // The popup owns its keys while it has the focus (list navigation, digits).
  function onKeyDown(e: KeyboardEvent) {
    const n = props.targets.length;
    if (e.key === "ArrowDown") setIndex((i) => (i + 1) % n);
    else if (e.key === "ArrowUp") setIndex((i) => (i + n - 1) % n);
    else if (e.key === "Enter") pick(props.targets[index()]);
    else if (e.key === "Escape") close();
    else if (/^[1-9]$/.test(e.key) && !e.ctrlKey && !e.metaKey && !e.altKey) pick(props.targets[Number(e.key) - 1]);
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div class="popup select-in-popup" tabIndex={-1} ref={root} role="dialog" aria-label={tr("tools.editorNavigation.selectIn")} onKeyDown={onKeyDown}>
        <div class="select-in-title">
          {tr("tools.editorNavigation.selectIn")} <span class="select-in-file">{props.path.split("/").pop()}</span>
        </div>
        <div class="select-in-list" role="listbox">
          <For each={props.targets}>
            {(t, i) => (
              <div
                class="select-in-row"
                classList={{ selected: index() === i() }}
                role="option"
                aria-selected={index() === i()}
                data-target={t.id}
                onMouseMove={() => setIndex(i())}
                onClick={() => pick(t)}
              >
                <span class="select-in-mnemonic">{i() < 9 ? `${i() + 1}.` : ""}</span>
                <span>{t.label}</span>
              </div>
            )}
          </For>
        </div>
      </div>
    </div>
  );
}

registerCommand({
  id: "SelectIn",
  title: actionById("SelectIn")?.title ?? "Select In…",
  category: "Navigate",
  run: () => openSelectIn(),
});

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "select-in";
  document.body.appendChild(host);
  render(
    () => (
      <Show when={session()} keyed>
        {(s) => <SelectInPopup targets={s.targets} path={s.ctx.path} />}
      </Show>
    ),
    host,
  );
}
