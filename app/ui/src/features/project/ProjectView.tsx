import { createEffect, createResource, createRoot, createSignal, For, on, Show, type Component } from "solid-js";
import { Dynamic } from "solid-js/web";
import { ChevronDown, ChevronRight, ChevronsDownUp, EllipsisVertical, LocateFixed } from "lucide-solid";
import { workspaceApi, type FileEntry } from "@/ipc/workspace";
import { workspace } from "@/state/workspace";
import { editor, isCustomTab, openFile } from "@/state/editor";
import { statusOf, dirHasChanges, dirHasConflicts } from "@/state/vcs";
import { FileIcon } from "@/shell/FileIcon";
import { getSetting, saveSetting } from "@/state/settings";
import { isToolWindowVisible } from "@/state/layout";
import { showResolvedMenu, type ResolvedEntry } from "@/features/contextMenu/state";
import { showFileMenu } from "./fileMenu";
import { navigate, type VisibleRow } from "./treeNavigation";
import { projectViewColor } from "@/features/scopes/model";

const [expanded, setExpanded] = createSignal<Set<string>>(new Set(["app", "app/Models"]));

const setOpen = (path: string, open: boolean) =>
  setExpanded((prev) => {
    const next = new Set(prev);
    open ? next.add(path) : next.delete(path);
    return next;
  });
const toggle = (path: string) => setOpen(path, !expanded().has(path));

/** Expand `dir` and all its parents so the entries below it are visible. */
export function revealPath(dir: string): void {
  if (!dir) return;
  const parts = dir.split("/");
  setExpanded((prev) => {
    const next = new Set(prev);
    parts.forEach((_, i) => next.add(parts.slice(0, i + 1).join("/")));
    return next;
  });
}

/** The rows being dragged and the folder they would be dropped on (Move by drag and drop). */
const [dragged, setDragged] = createSignal<string[]>([]);
const [dropTarget, setDropTarget] = createSignal<string | null>(null);

/** Whether `path` can be dropped into the folder `dir` ("" is the project root). */
export function canDrop(path: string, dir: string): boolean {
  const parent = path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
  return !path.includes("://") && dir !== parent && dir !== path && !dir.startsWith(`${path}/`);
}

const canDropAll = (dir: string) => dragged().length > 0 && dragged().every((p) => canDrop(p, dir));

function dropInto(dir: string): void {
  const paths = dragged();
  setDragged([]);
  setDropTarget(null);
  if (!paths.length || !paths.every((p) => canDrop(p, dir))) return;
  void import("@/features/codeintel/moveRefactorings").then((m) => m.moveEntries(paths, dir));
}

/** Tree selection, independent of the active editor tab (like PHPStorm). */
const [selected, setSelected] = createSignal<string | null>(null);

/** Rows selected together with `selected` (Ctrl+click), e.g. the two files of Compare Files. */
const [alsoSelected, setAlsoSelected] = createSignal<string[]>([]);

/** Every selected row, the lead row first. */
export const selectedPaths = (): string[] => {
  const lead = selected();
  return lead ? [lead, ...alsoSelected().filter((p) => p !== lead)] : alsoSelected();
};

/** Select one row only (clears a Ctrl+click selection). */
function selectOnly(path: string | null): void {
  setAlsoSelected([]);
  setSelected(path);
}

/** Ctrl+click: add the row to the selection or take it out. */
function toggleSelected(path: string): void {
  const all = selectedPaths();
  const next = all.includes(path) ? all.filter((p) => p !== path) : [path, ...all];
  setSelected(next[0] ?? null);
  setAlsoSelected(next.slice(1));
}

/** Select a row (Select in Project View); call `revealPath` for its folder first. */
export const selectPath = (path: string) => selectOnly(path);

const rowOf = (path: string) => document.querySelector<HTMLElement>(`.project-tree .tree-row[data-path="${CSS.escape(path)}"]`);

/**
 * Select a file in the tree: expand its folders, select its row and scroll it into view once
 * the folders' listings are loaded (Select In… › Project View, Always Select Opened File).
 */
export function selectInTree(path: string): void {
  revealPath(path.split("/").slice(0, -1).join("/"));
  selectOnly(path);
  let frames = 0;
  const scroll = () => {
    const row = rowOf(path);
    if (row) row.scrollIntoView({ block: "nearest" });
    else if (++frames < 60) requestAnimationFrame(scroll);
  };
  requestAnimationFrame(scroll);
}

const ALWAYS_SELECT = "projectView.autoscrollFromSource";
const isProjectFile = (path: string) => !path.includes("://") && !isCustomTab(path);

// Always Select Opened File (PHPStorm's "Autoscroll from Source"): the tree follows the
// active editor while the Project tool window is shown.
createRoot(() =>
  createEffect(
    on([() => editor.active, () => getSetting<boolean>(ALWAYS_SELECT, false), () => isToolWindowVisible("project")], ([path, on, shown]) => {
      if (path && on && shown && isProjectFile(path)) selectInTree(path);
    }),
  ),
);

/** The tool window header: Select Opened File, Collapse All and the Options menu. */
export function ProjectActions() {
  const options = (e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const toggle = (key: string, label: string, fallback: boolean): ResolvedEntry => {
      const value = getSetting<boolean>(key, fallback);
      return { kind: "action", label, checkable: true, checked: value, run: () => void saveSetting(key, !value) };
    };
    showResolvedMenu(
      [
        toggle(ALWAYS_SELECT, "Always Select Opened File", false),
        toggle("projectView.openFilesWithSingleClick", "Open Files with Single Click", true),
      ],
      r.left,
      r.bottom + 2,
    );
  };
  return (
    <>
      <button
        class="icon-btn"
        title="Select Opened File"
        aria-label="Select Opened File"
        disabled={!editor.active || !isProjectFile(editor.active)}
        onClick={() => editor.active && selectInTree(editor.active)}
      >
        <LocateFixed size={15} />
      </button>
      <button class="icon-btn" title="Collapse All" aria-label="Collapse All" onClick={() => setExpanded(new Set<string>())}>
        <ChevronsDownUp size={15} />
      </button>
      <button class="icon-btn" title="Options" aria-label="Options" onClick={options}>
        <EllipsisVertical size={15} />
      </button>
    </>
  );
}

const singleClickOpens = () => getSetting<boolean>("projectView.openFilesWithSingleClick", true);

/** Double click (or single click with the setting on): expand/collapse folders, open files. */
function activate(entry: FileEntry): void {
  if (entry.kind === "dir") toggle(entry.path);
  else void openFile(entry.path);
}

function TreeNode(props: { entry: FileEntry; depth: number }) {
  const isDir = () => props.entry.kind === "dir";
  const isOpen = () => expanded().has(props.entry.path);
  const status = () => (isDir() ? (dirHasConflicts(props.entry.path) ? "conflict" : dirHasChanges(props.entry.path) ? "modified" : undefined) : statusOf(props.entry.path));
  const color = () => projectViewColor(props.entry.path, isDir());
  return (
    <>
      <div
        class="tree-row"
        classList={{
          selected: selected() === props.entry.path || alsoSelected().includes(props.entry.path),
          "active-file": editor.active === props.entry.path,
          ignored: !!props.entry.ignored,
          "file-colored": !!color(),
          "drop-target": isDir() && dropTarget() === props.entry.path,
        }}
        style={{ "padding-left": `${8 + props.depth * 16}px`, "--file-color": color() }}
        data-path={props.entry.path}
        data-dir={isDir() ? "" : undefined}
        data-expanded={isOpen() ? "" : undefined}
        data-depth={props.depth}
        draggable={true}
        onDragStart={(e) => {
          const all = selectedPaths();
          setDragged(all.includes(props.entry.path) ? all : [props.entry.path]);
          e.dataTransfer?.setData("text/plain", props.entry.path);
          if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
        }}
        onDragEnd={() => {
          setDragged([]);
          setDropTarget(null);
        }}
        onDragOver={(e) => {
          const dir = isDir() ? props.entry.path : props.entry.path.split("/").slice(0, -1).join("/");
          if (!canDropAll(dir)) return;
          e.preventDefault();
          if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
          setDropTarget(dir);
        }}
        onDragLeave={() => setDropTarget(null)}
        onDrop={(e) => {
          e.preventDefault();
          dropInto(isDir() ? props.entry.path : props.entry.path.split("/").slice(0, -1).join("/"));
        }}
        onClick={(e) => {
          if (e.ctrlKey || e.metaKey) return toggleSelected(props.entry.path);
          selectOnly(props.entry.path);
          if (singleClickOpens()) activate(props.entry);
        }}
        onDblClick={() => {
          if (!singleClickOpens()) activate(props.entry);
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          e.stopPropagation();
          // A row of a multiple selection keeps it (Compare Files acts on both).
          if (!selectedPaths().includes(props.entry.path)) selectOnly(props.entry.path);
          showFileMenu(props.entry, e.clientX, e.clientY);
        }}
      >
        <span
          class="tree-chevron"
          onClick={(e) => {
            // The arrow toggles on a single click, as in PHPStorm.
            if (!isDir()) return;
            e.stopPropagation();
            selectOnly(props.entry.path);
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
export const projectTree = { expanded, setOpen, toggle, selected, setSelected: selectOnly, selectedPaths, singleClickOpens };

export function ProjectView() {
  let root!: HTMLDivElement;
  const onKeyDown = (e: KeyboardEvent) => {
    const action = navigate(visibleRows(root), selected(), e.key);
    if (action.type === "none") return;
    e.preventDefault();
    if (action.type === "select") {
      selectOnly(action.path);
      root.querySelector(`.tree-row[data-path="${CSS.escape(action.path)}"]`)?.scrollIntoView({ block: "nearest" });
    } else if (action.type === "expand") setOpen(action.path, true);
    else if (action.type === "collapse") setOpen(action.path, false);
    else void openFile(action.path);
  };
  return (
    <div
      class="project-tree"
      tabIndex={0}
      ref={root}
      onKeyDown={onKeyDown}
      onContextMenu={(e) => {
        // Rows handle their own menu; the root row and the empty area act on the project root.
        e.preventDefault();
        selectOnly(null);
        showFileMenu(null, e.clientX, e.clientY);
      }}
    >
      <Show when={workspace()}>
        {(ws) => (
          <>
            <div
              class="tree-row tree-root"
              classList={{ "drop-target": dropTarget() === "" }}
              style={{ "padding-left": "8px" }}
              onDragOver={(e) => {
                if (!canDropAll("")) return;
                e.preventDefault();
                setDropTarget("");
              }}
              onDragLeave={() => setDropTarget(null)}
              onDrop={(e) => {
                e.preventDefault();
                dropInto("");
              }}
            >
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
