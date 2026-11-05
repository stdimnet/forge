/**
 * Project tool window (Alt+1). The rows of a folder come from Rust (`project_tree_list`):
 * excluded folders (orange, or hidden without "Show Excluded Files"), File Nesting, Compact
 * Middle Packages, Folders Always on Top and Sort by Type. With "Show Members" PHP,
 * JavaScript, TypeScript and Lua files expand to their structure (the Structure view's
 * providers).
 */
import { createEffect, createResource, createRoot, createSignal, For, on, Show, type Component } from "solid-js";
import { Dynamic } from "solid-js/web";
import { ChevronsDownUp, EllipsisVertical, LocateFixed } from "lucide-solid";
import { workspaceApi, type FileEntry } from "@/ipc/workspace";
import { projectStructureApi, type TreeRow } from "@/ipc/projectStructure";
import { onProjectSwitch, workspace } from "@/state/workspace";
import { activeTab, editor, isCustomTab } from "@/state/editor";
import { openFileReported } from "@/features/navigation/openReported";
import { statusOf, dirHasChanges, dirHasConflicts } from "@/state/vcs";
import { FileIcon } from "@/shell/FileIcon";
import { TreeArrow } from "@/shell/TreeArrow";
import { isHollowFolder } from "@/features/iconThemes/state";
import { getSetting, saveSetting } from "@/state/settings";
import { isToolWindowVisible } from "@/state/layout";
import { showResolvedMenu, type ResolvedEntry } from "@/features/contextMenu/state";
import { showFileMenu } from "./fileMenu";
import { navigate, type VisibleRow } from "./treeNavigation";
import { projectViewColor } from "@/features/scopes/model";
import { showMembers, viewOptions, VIEW_KEYS } from "@/features/projectStructure/model";
import { showFileNestingDialog } from "@/features/projectStructure/FileNestingDialog";
import { caretNodeId, hasFileStructure, loadFileStructure, revealStructureNode } from "@/features/php/StructureView";
import { SymbolIcon } from "@/features/php/SymbolIcon";
import type { FileStructure, StructureNode } from "@/features/php/api";
import { t } from "@/i18n";

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
    if (row) return row.scrollIntoView({ block: "nearest" });
    // A nested file (File Nesting) shows once its parent file is expanded.
    const parent = [...document.querySelectorAll<HTMLElement>(".project-tree .tree-row[data-nested]")].find((r) =>
      r.dataset.nested!.split("\n").includes(path),
    );
    if (parent?.dataset.path) setOpen(parent.dataset.path, true);
    if (++frames < 60) requestAnimationFrame(scroll);
  };
  requestAnimationFrame(scroll);
}

const ALWAYS_SELECT = "projectView.autoscrollFromSource";
const isProjectFile = (path: string) => !path.includes("://") && !isCustomTab(path);

// Always Select Opened File (PHPStorm's "Autoscroll from Source"): the tree follows the
// active editor while the Project tool window is shown.
createRoot(() => {
  createEffect(
    on([() => editor.active, () => getSetting<boolean>(ALWAYS_SELECT, false), () => isToolWindowVisible("project")], ([path, on, shown]) => {
      if (path && on && shown && isProjectFile(path)) selectInTree(path);
    }),
  );
  // With Show Members the member at the caret is selected as the caret moves.
  createEffect(
    on([() => activeTab()?.path, () => activeTab()?.cursor.line, showMembers, () => getSetting<boolean>(ALWAYS_SELECT, false)], ([path, , members, on]) => {
      if (!path || !members || !on || !isProjectFile(path) || !hasFileStructure(path)) return;
      setOpen(path, true);
      const s = memberStructures().get(path);
      const id = s ? caretNodeId(s) : null;
      if (id == null) return;
      selectOnly(memberKey(path, id));
      requestAnimationFrame(() => rowOf(memberKey(path, id))?.scrollIntoView({ block: "nearest" }));
    }),
  );
});

/** The tool window header: Select Opened File, Collapse All and the Options menu. */
export function ProjectActions() {
  const options = (e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const toggle = (key: string, label: string, fallback: boolean): ResolvedEntry => {
      const value = getSetting<boolean>(key, fallback);
      return { kind: "action", label, checkable: true, checked: value, run: () => void saveSetting(key, !value) };
    };
    const group = (label: string, items: ResolvedEntry[]): ResolvedEntry => ({ kind: "submenu", label, items });
    showResolvedMenu(
      [
        group(t("project.view.behavior"), [
          toggle(ALWAYS_SELECT, t("project.view.alwaysSelect"), false),
          toggle("projectView.openFilesWithSingleClick", t("project.view.singleClick"), true),
        ]),
        group(t("project.view.appearance"), [
          toggle(VIEW_KEYS.showMembers, t("project.view.showMembers"), false),
          toggle(VIEW_KEYS.showExcluded, t("project.view.showExcluded"), true),
          toggle(VIEW_KEYS.compact, t("project.view.compact"), false),
          "separator",
          { kind: "action", label: t("project.view.fileNesting"), run: showFileNestingDialog },
        ]),
        group(t("project.view.sort"), [
          { kind: "action", label: t("project.view.byName"), checkable: true, checked: !getSetting<boolean>(VIEW_KEYS.sortByType, false), run: () => void saveSetting(VIEW_KEYS.sortByType, false) },
          { kind: "action", label: t("project.view.byType"), checkable: true, checked: getSetting<boolean>(VIEW_KEYS.sortByType, false), run: () => void saveSetting(VIEW_KEYS.sortByType, true) },
          "separator",
          toggle(VIEW_KEYS.foldersOnTop, t("project.view.foldersOnTop"), true),
        ]),
      ],
      r.left,
      r.bottom + 2,
    );
  };
  return (
    <>
      <button
        class="icon-btn"
        title={t("project.view.selectOpened")}
        aria-label={t("project.view.selectOpened")}
        disabled={!editor.active || !isProjectFile(editor.active)}
        onClick={() => editor.active && selectInTree(editor.active)}
      >
        <LocateFixed size={15} />
      </button>
      <button class="icon-btn" title={t("project.view.collapseAll")} aria-label={t("project.view.collapseAll")} onClick={() => setExpanded(new Set<string>())}>
        <ChevronsDownUp size={15} />
      </button>
      <button class="icon-btn" title={t("project.view.options")} aria-label={t("project.view.options")} onClick={options}>
        <EllipsisVertical size={15} />
      </button>
    </>
  );
}

const singleClickOpens = () => getSetting<boolean>("projectView.openFilesWithSingleClick", true);

/** Double click (or single click with the setting on): expand/collapse folders, open files. */
function activate(entry: FileEntry): void {
  if (entry.kind === "dir") toggle(entry.path);
  else void openFileReported(entry.path);
}

/** The entry a row stands for (menus, drag and drop). */
const entryOf = (row: TreeRow): FileEntry => ({
  name: row.path.split("/").pop() ?? row.name,
  path: row.path,
  kind: row.kind,
  ignored: row.ignored,
  excluded: row.excluded,
});

/** Element id of a row (aria-activedescendant), stable per path or member key. */
const rowIds = new Map<string, string>();
const treeRowId = (key: string): string => {
  let id = rowIds.get(key);
  if (!id) rowIds.set(key, (id = `project-row-${rowIds.size + 1}`));
  return id;
};

/** Show Members: the loaded structure per file, for the member at the caret. */
const [memberStructures, setMemberStructures] = createSignal<Map<string, FileStructure>>(new Map());
/** A member row's key: the file and the node's index path (`app/User.php#0/2`). */
const memberKey = (file: string, id: string) => `${file}#${id}`;

function MemberRows(props: { file: string; nodes: StructureNode[]; parent: string; depth: number }) {
  return (
    <For each={props.nodes.map((node, i) => ({ node, id: props.parent ? `${props.parent}/${i}` : String(i) })).filter((n) => !n.node.inheritedFrom)}>
      {({ node, id }) => {
        const key = memberKey(props.file, id);
        const kids = () => (node.children ?? []).length > 0;
        const open = () => expanded().has(key);
        return (
          <>
            <div
              class="tree-row project-member-row"
              classList={{ selected: selected() === key, deprecated: !!node.deprecated }}
              style={{ "padding-left": `${8 + props.depth * 16}px` }}
              data-member={key}
              id={treeRowId(key)}
              role="treeitem"
              aria-level={props.depth + 1}
              aria-selected={selected() === key}
              aria-expanded={kids() ? open() : undefined}
              title={node.detail || undefined}
              onClick={() => {
                selectOnly(key);
                revealStructureNode(props.file, node);
              }}
            >
              <span
                class="tree-chevron"
                onClick={(e) => {
                  if (!kids()) return;
                  e.stopPropagation();
                  toggle(key);
                }}
              >
                <Show when={kids()}>
                  <TreeArrow open={open()} />
                </Show>
              </span>
              <SymbolIcon kind={node.kind} visibility={node.visibility} modifiers={node.modifiers} />
              <span class="tree-label">{node.name}</span>
              <Show when={node.detail}>
                <span class="tree-hint">{node.detail}</span>
              </Show>
            </div>
            <Show when={kids() && open()}>
              <MemberRows file={props.file} nodes={node.children ?? []} parent={id} depth={props.depth + 1} />
            </Show>
          </>
        );
      }}
    </For>
  );
}

/** The members of a file (Show Members), from the open editor's text or the file on disk. */
function FileMembers(props: { path: string; depth: number }) {
  const [structure] = createResource(
    () => `${dirRevisions()[props.path] ?? 0}:${props.path}:${editor.tabs.find((t) => t.path === props.path)?.saved.length ?? -1}`,
    async () => {
      const content = editor.tabs.find((t) => t.path === props.path)?.content ?? (await workspaceApi.readFile(props.path));
      const s = await loadFileStructure(props.path, content).catch(() => null);
      if (s)
        setMemberStructures((m) => {
          const next = new Map(m);
          next.set(props.path, s);
          return next;
        });
      return s;
    },
  );
  return (
    <Show when={structure()}>
      {(s) => <MemberRows file={props.path} nodes={s().nodes} parent="" depth={props.depth} />}
    </Show>
  );
}

/**
 * Extra classes and a hint for tree rows, e.g. PHP source roots ("Mark Directory as"): a
 * decorator returns `{ class, hint }` for a path, or nothing.
 */
type RowDecorator = (path: string, dir: boolean) => { class?: string; hint?: string } | undefined;
const [decorators, setDecorators] = createSignal<RowDecorator[]>([]);

export function registerRowDecorator(d: RowDecorator): void {
  setDecorators((prev) => [...prev, d]);
}

function TreeNode(props: { row: TreeRow; depth: number }) {
  const entry = () => entryOf(props.row);
  const isDir = () => props.row.kind === "dir";
  const nested = () => props.row.nested ?? [];
  const members = () => !isDir() && showMembers() && hasFileStructure(props.row.path);
  const expandable = () => isDir() || nested().length > 0 || members();
  const decoration = () => {
    for (const d of decorators()) {
      const r = d(props.row.path, isDir());
      if (r) return r;
    }
    return undefined;
  };
  const isOpen = () => expanded().has(props.row.path);
  const status = () => (isDir() ? (dirHasConflicts(props.row.path) ? "conflict" : dirHasChanges(props.row.path) ? "modified" : undefined) : statusOf(props.row.path));
  const color = () => projectViewColor(props.row.path, isDir());
  return (
    <>
      <div
        class={`tree-row ${decoration()?.class ?? ""}`}
        classList={{
          selected: selected() === props.row.path || alsoSelected().includes(props.row.path),
          "active-file": editor.active === props.row.path,
          ignored: !!props.row.ignored,
          excluded: !!props.row.excluded,
          "file-colored": !!color(),
          "drop-target": isDir() && dropTarget() === props.row.path,
        }}
        style={{ "padding-left": `${8 + props.depth * 16}px`, "--file-color": color() }}
        data-path={props.row.path}
        data-dir={isDir() ? "" : undefined}
        data-excluded={props.row.excluded ? "" : undefined}
        data-nested={nested().length ? nested().map((n) => n.path).join("\n") : undefined}
        title={props.row.compacted ? props.row.path : undefined}
        data-expanded={isOpen() ? "" : undefined}
        data-depth={props.depth}
        id={treeRowId(props.row.path)}
        role="treeitem"
        aria-level={props.depth + 1}
        aria-selected={selected() === props.row.path || alsoSelected().includes(props.row.path)}
        aria-expanded={expandable() ? isOpen() : undefined}
        aria-label={status() ? t("a11y.tree.vcsRow", { name: props.row.name, status: status()! }) : undefined}
        draggable={true}
        onDragStart={(e) => {
          const all = selectedPaths();
          setDragged(all.includes(props.row.path) ? all : [props.row.path]);
          e.dataTransfer?.setData("text/plain", props.row.path);
          if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
        }}
        onDragEnd={() => {
          setDragged([]);
          setDropTarget(null);
        }}
        onDragOver={(e) => {
          const dir = isDir() ? props.row.path : props.row.path.split("/").slice(0, -1).join("/");
          if (!canDropAll(dir)) return;
          e.preventDefault();
          if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
          setDropTarget(dir);
        }}
        onDragLeave={() => setDropTarget(null)}
        onDrop={(e) => {
          e.preventDefault();
          dropInto(isDir() ? props.row.path : props.row.path.split("/").slice(0, -1).join("/"));
        }}
        onClick={(e) => {
          if (e.ctrlKey || e.metaKey) return toggleSelected(props.row.path);
          selectOnly(props.row.path);
          if (singleClickOpens()) activate(entry());
        }}
        onDblClick={() => {
          if (!singleClickOpens()) activate(entry());
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          e.stopPropagation();
          // A row of a multiple selection keeps it (Compare Files acts on both).
          if (!selectedPaths().includes(props.row.path)) selectOnly(props.row.path);
          showFileMenu(entry(), e.clientX, e.clientY);
        }}
      >
        <span
          class="tree-chevron"
          onClick={(e) => {
            // The arrow toggles on a single click, as in PHPStorm.
            if (!expandable()) return;
            e.stopPropagation();
            selectOnly(props.row.path);
            toggle(props.row.path);
          }}
        >
          <Show when={expandable()}>
            <TreeArrow open={isOpen()} />
          </Show>
        </span>
        <FileIcon
          name={entry().name}
          path={props.row.path}
          dir={isDir()}
          open={isOpen() && isDir()}
          hollow={isDir() && isHollowFolder(props.row.path)}
          dimmed={props.row.ignored}
        />
        <span class={`tree-label ${status() ? `vcs-${status()}` : ""}`}>{props.row.name}</span>
        <Show when={decoration()?.hint}>
          <span class="tree-hint">{decoration()?.hint}</span>
        </Show>
      </div>
      <Show when={isDir() && isOpen()}>
        <DirChildren path={props.row.path} depth={props.depth + 1} />
      </Show>
      <Show when={!isDir() && isOpen()}>
        <For each={nested()}>{(child) => <TreeNode row={child} depth={props.depth + 1} />}</For>
        <Show when={members()}>
          <FileMembers path={props.row.path} depth={props.depth + 1} />
        </Show>
      </Show>
    </>
  );
}

/** Bumped per directory when its listing changed on disk, so the tree re-reads it. */
const [dirRevisions, setDirRevisions] = createSignal<Record<string, number>>({});

/** Re-read the listings of these directories ("" is the project root). */
/** Reload every folder the Project view has listed (Repair IDE, ignored files changed). */
export function refreshAllDirs(): void {
  refreshDirs(["", ...Object.keys(dirRevisions())]);
}

export function refreshDirs(dirs: string[]): void {
  if (!dirs.length) return;
  setDirRevisions((prev) => {
    const next = { ...prev };
    for (const d of dirs) next[d] = (next[d] ?? 0) + 1;
    return next;
  });
}

/** Bumped when the excluded folders changed: every listing is read again. */
const [structureRevision, setStructureRevision] = createSignal(0);

/** Re-read every listing (the excluded folders or the view options changed). */
export const refreshTree = () => setStructureRevision((n) => n + 1);

function DirChildren(props: { path: string; depth: number }) {
  const [rows] = createResource(
    // A string key, so a change elsewhere in the tree does not refetch this directory.
    () => `${structureRevision()}:${dirRevisions()[props.path] ?? 0}:${JSON.stringify(viewOptions())}:${props.path}`,
    () => projectStructureApi.treeList(props.path, viewOptions()),
  );
  return <For each={rows() ?? []}>{(row) => <TreeNode row={row} depth={props.depth} />}</For>;
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

/**
 * Another project in this window (e.g. a Code With Me guest got the host's project): its
 * tree starts collapsed and unselected, and every listing is read again.
 */
export function resetProjectTree(): void {
  setExpanded(new Set<string>());
  setSelected(null);
  setAlsoSelected([]);
  setDragged([]);
  setDropTarget(null);
  setDirRevisions({});
  setStructureRevision((n) => n + 1);
}

onProjectSwitch(resetProjectTree);

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
    else void openFileReported(action.path);
  };
  return (
    <div
      class="project-tree"
      tabIndex={0}
      ref={root}
      role="tree"
      aria-label={t("toolwindow.project.title")}
      aria-multiselectable="true"
      aria-activedescendant={selected() ? treeRowId(selected()!) : undefined}
      onKeyDown={onKeyDown}
      onContextMenu={(e) => {
        // Rows handle their own menu; the root row and the empty area act on the project root.
        e.preventDefault();
        selectOnly(null);
        showFileMenu(null, e.clientX, e.clientY);
      }}
    >
      {/* Keyed by the root: another project gets a new tree, not the old one's listings. */}
      <Show when={workspace()?.root} keyed>
        {(root) => (
          <>
            <div
              class="tree-row tree-root"
              classList={{ "drop-target": dropTarget() === "" }}
              style={{ "padding-left": "8px" }}
              role="treeitem"
              aria-level={1}
              aria-expanded="true"
              aria-selected={selected() === null}
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
                <TreeArrow open />
              </span>
              <FileIcon name={workspace()?.name ?? ""} dir open root />
              <span class="tree-label strong">{workspace()?.name}</span>
              <span class="tree-hint">{workspace()?.collab?.label ?? root}</span>
            </div>
            <DirChildren path="" depth={1} />
            <For each={sections()}>{(section) => <Dynamic component={section.component} />}</For>
          </>
        )}
      </Show>
    </div>
  );
}
