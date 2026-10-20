import { createResource, createSignal, For, Show, type Component } from "solid-js";
import { Dynamic } from "solid-js/web";
import { ChevronDown, ChevronRight } from "lucide-solid";
import { workspaceApi, type FileEntry } from "@/ipc/workspace";
import { workspace } from "@/state/workspace";
import { editor, openFile } from "@/state/editor";
import { statusOf, dirHasChanges } from "@/state/vcs";
import { FileIcon } from "@/shell/FileIcon";
import { getSetting } from "@/state/settings";
import { navigate, type VisibleRow } from "./treeNavigation";

const [expanded, setExpanded] = createSignal<Set<string>>(new Set(["app", "app/Models"]));

const setOpen = (path: string, open: boolean) =>
  setExpanded((prev) => {
    const next = new Set(prev);
    open ? next.add(path) : next.delete(path);
    return next;
  });
const toggle = (path: string) => setOpen(path, !expanded().has(path));

/** Tree selection, independent of the active editor tab (like PHPStorm). */
const [selected, setSelected] = createSignal<string | null>(null);

const singleClickOpens = () => getSetting<boolean>("projectView.openFilesWithSingleClick", true);

/** Double click (or single click with the setting on): expand/collapse folders, open files. */
function activate(entry: FileEntry): void {
  if (entry.kind === "dir") toggle(entry.path);
  else void openFile(entry.path);
}

function TreeNode(props: { entry: FileEntry; depth: number }) {
  const isDir = () => props.entry.kind === "dir";
  const isOpen = () => expanded().has(props.entry.path);
  const status = () => (isDir() ? (dirHasChanges(props.entry.path) ? "modified" : undefined) : statusOf(props.entry.path));
  return (
    <>
      <div
        class="tree-row"
        classList={{ selected: selected() === props.entry.path, "active-file": editor.active === props.entry.path, ignored: !!props.entry.ignored }}
        style={{ "padding-left": `${8 + props.depth * 16}px` }}
        data-path={props.entry.path}
        data-dir={isDir() ? "" : undefined}
        data-expanded={isOpen() ? "" : undefined}
        data-depth={props.depth}
        onClick={() => {
          setSelected(props.entry.path);
          if (singleClickOpens()) activate(props.entry);
        }}
        onDblClick={() => {
          if (!singleClickOpens()) activate(props.entry);
        }}
      >
        <span
          class="tree-chevron"
          onClick={(e) => {
            // The arrow toggles on a single click, as in PHPStorm.
            if (!isDir()) return;
            e.stopPropagation();
            setSelected(props.entry.path);
            toggle(props.entry.path);
          }}
        >
          <Show when={isDir()}>{isOpen() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</Show>
        </span>
        <FileIcon name={props.entry.name} dir={isDir()} open={isOpen()} dimmed={props.entry.ignored} />
        <span class={`tree-label ${status() ? `vcs-${status()}` : ""}`}>{props.entry.name}</span>
      </div>
      <Show when={isDir() && isOpen()}>
        <DirChildren path={props.entry.path} depth={props.depth + 1} />
      </Show>
    </>
  );
}

/** Bumped per directory when its listing changed on disk, so the tree re-reads it. */
const [dirRevisions, setDirRevisions] = createSignal<Record<string, number>>({});

/** Re-read the listings of these directories ("" is the project root). */
export function refreshDirs(dirs: string[]): void {
  if (!dirs.length) return;
  setDirRevisions((prev) => {
    const next = { ...prev };
    for (const d of dirs) next[d] = (next[d] ?? 0) + 1;
    return next;
  });
}

function DirChildren(props: { path: string; depth: number }) {
  const [entries] = createResource(
    // A string key, so a change elsewhere in the tree does not refetch this directory.
    () => `${dirRevisions()[props.path] ?? 0}:${props.path}`,
    () => workspaceApi.listDir(props.path),
  );
  return <For each={entries() ?? []}>{(e) => <TreeNode entry={e} depth={props.depth} />}</For>;
}

/** Rows currently rendered, in display order, read from the DOM. */
function visibleRows(root: HTMLElement): VisibleRow[] {
  return [...root.querySelectorAll<HTMLElement>(".tree-row[data-path]")].map((el) => ({
    path: el.dataset.path!,
    isDir: el.dataset.dir !== undefined,
    expanded: el.dataset.expanded !== undefined,
    depth: Number(el.dataset.depth ?? 0),
  }));
}

/**
 * Top-level nodes below the project root, like PHPStorm's "Scratches and Consoles". Their
 * rows are `.tree-row`s with a `data-path` outside the project (e.g. `scratch://name`) and use
 * the tree's expansion and selection (`projectTree`), so keyboard navigation spans them.
 */
const [sections, setSections] = createSignal<{ id: string; order: number; component: Component }[]>([]);

export function registerProjectViewSection(section: { id: string; order: number; component: Component }): void {
  setSections((prev) => [...prev.filter((s) => s.id !== section.id), section].sort((a, b) => a.order - b.order));
}

/** Expansion and selection of the Project tree, for sections. */
export const projectTree = { expanded, setOpen, toggle, selected, setSelected, singleClickOpens };

export function ProjectView() {
  let root!: HTMLDivElement;
  const onKeyDown = (e: KeyboardEvent) => {
    const action = navigate(visibleRows(root), selected(), e.key);
    if (action.type === "none") return;
    e.preventDefault();
    if (action.type === "select") {
      setSelected(action.path);
      root.querySelector(`.tree-row[data-path="${CSS.escape(action.path)}"]`)?.scrollIntoView({ block: "nearest" });
    } else if (action.type === "expand") setOpen(action.path, true);
    else if (action.type === "collapse") setOpen(action.path, false);
    else void openFile(action.path);
  };
  return (
    <div class="project-tree" tabIndex={0} ref={root} onKeyDown={onKeyDown}>
      <Show when={workspace()}>
        {(ws) => (
          <>
            <div class="tree-row tree-root" style={{ "padding-left": "8px" }}>
              <span class="tree-chevron">
                <ChevronDown size={14} />
              </span>
              <FileIcon name={ws().name} dir open />
              <span class="tree-label strong">{ws().name}</span>
              <span class="tree-hint">{ws().root}</span>
            </div>
            <DirChildren path="" depth={1} />
            <For each={sections()}>{(section) => <Dynamic component={section.component} />}</For>
          </>
        )}
      </Show>
    </div>
  );
}
