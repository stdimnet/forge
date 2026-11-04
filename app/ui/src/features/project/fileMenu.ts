/**
 * Context menu of the Project view (PHPStorm: New, Copy Path, Find in Files, Rename, Delete,
 * Reload from Disk). Folder and file operations go through the workspace IPC commands, which
 * reject paths outside the project.
 */
import { askConfirm, askName } from "@/features/contextMenu/dialogs";
import { item, providedEntries, separator, showContextMenu, submenu, type MenuEntry } from "@/features/contextMenu/state";
import { workspaceApi, type FileEntry } from "@/ipc/workspace";
import { showBalloon } from "@/keymap";
import { closeTab, editor, isDirty, openFile, saveFile } from "@/state/editor";
import { isCollabGuest, workspace } from "@/state/workspace";
import { guestCanEdit } from "@/features/code-with-me/guest";
import { projectTree, refreshDirs, revealPath } from "./ProjectView";
import { openFindInFiles } from "@/features/find/state";
import { copyEntries, moveEntries, safeDeleteEntries } from "@/features/codeintel/moveRefactorings";
import { t } from "@/i18n";

const parentOf = (path: string) => path.split("/").slice(0, -1).join("/");
const join = (dir: string, name: string) => (dir ? `${dir}/${name}` : name);

/** The folder a New action creates in: the folder itself, or the folder of a file. */
const targetDir = (entry: FileEntry | null) => (!entry ? "" : entry.kind === "dir" ? entry.path : parentOf(entry.path));

/** Names that would escape the folder or cannot be a single path segment. */
export function validateName(name: string): string | null {
  if (name === "." || name === "..") return t("project.name.invalid", { name });
  if (/[\\\0]/.test(name)) return t("project.name.badChars");
  return null;
}

/** Like `validateName`, but a new entry may name nested folders (`app/Models/Order.php`). */
export function validateNewPath(name: string): string | null {
  if (name.startsWith("/")) return t("project.path.absolute");
  const bad = name.split("/").find((s) => s === "" || s === "." || s === ".." || validateName(s));
  return bad === undefined ? null : bad === "" ? t("project.path.emptySegment") : (validateName(bad) ?? t("project.path.badSegment", { segment: bad }));
}

function fail(what: string, e: unknown): void {
  showBalloon(t("project.failed", { what }), String(e instanceof Error ? e.message : e));
}

const affectedTabs = (path: string) => editor.tabs.filter((t) => !t.view && (t.path === path || t.path.startsWith(`${path}/`)));

async function newEntry(dir: string, kind: "file" | "dir"): Promise<void> {
  const where = dir || workspace()?.name || "project";
  const name = await askName({
    title: kind === "file" ? t("project.newFile") : t("project.newDirectory"),
    label: t("project.nameIn", { where }),
    initial: "",
    confirm: t("common.ok"),
    validate: validateNewPath,
  });
  if (!name) return;
  const path = join(dir, name);
  try {
    if (kind === "file") await workspaceApi.createFile(path);
    else await workspaceApi.createDir(path);
  } catch (e) {
    return fail(kind === "file" ? t("project.creatingFile") : t("project.creatingDirectory"), e);
  }
  refreshDirs([dir, parentOf(path)]);
  revealPath(kind === "file" ? parentOf(path) : path);
  if (kind === "file") await openFile(path);
}

/**
 * File | New | File / Directory (`NewFile`, `NewDir`): in the folder selected in the Project
 * view, else the folder of the active editor, else the project root, like PHPStorm.
 */
export async function newEntryFromSelection(kind: "file" | "dir"): Promise<void> {
  await newEntry(await newTargetDir(), kind);
}

/**
 * The folder File | New creates in: the folder selected in the Project view (or the folder
 * of the selected file), else the folder of the active editor, else the project root.
 */
export async function newTargetDir(): Promise<string> {
  const active = editor.active && !editor.active.includes(":") ? editor.active : null;
  const path = projectTree.selected() ?? active;
  if (!path) return "";
  const entry = await workspaceApi
    .listDir(parentOf(path))
    .then((entries) => entries.find((e) => e.path === path) ?? null)
    .catch(() => null);
  return entry ? targetDir(entry) : parentOf(path);
}

async function rename(entry: FileEntry): Promise<void> {
  const name = await askName({
    title: entry.kind === "dir" ? t("project.renameDirectory") : t("project.renameFile"),
    label: t("project.newName"),
    initial: entry.name,
    confirm: t("project.rename"),
    validate: (n) => validateName(n) ?? (n.includes("/") ? t("project.noSlash") : null),
  });
  if (!name || name === entry.name) return;
  const target = join(parentOf(entry.path), name);
  const tabs = affectedTabs(entry.path);
  const wasActive = editor.active;
  try {
    // Unsaved edits go to disk first, so the renamed file keeps them.
    for (const t of tabs.filter(isDirty)) await saveFile(t.path);
    await workspaceApi.rename(entry.path, target);
  } catch (e) {
    return fail(t("project.renaming"), e);
  }
  const moved = tabs.map((t) => t.path);
  for (const p of moved) closeTab(p);
  for (const p of moved) await openFile(target + p.slice(entry.path.length)).catch(() => undefined);
  if (wasActive && moved.includes(wasActive)) await openFile(target + wasActive.slice(entry.path.length)).catch(() => undefined);
  refreshDirs([parentOf(entry.path)]);
  revealPath(parentOf(entry.path));
}

/**
 * Rename (`RenameElement`, Shift+F6) in the Project view and Refactor | Rename File
 * (`RenameFile`): the selected entry, else `path`.
 */
export async function renameEntryAt(path: string | null): Promise<void> {
  if (!path) return;
  const entry = await workspaceApi
    .listDir(parentOf(path))
    .then((entries) => entries.find((e) => e.path === path) ?? null)
    .catch(() => null);
  if (entry) await rename(entry);
}

export const renameSelection = () => renameEntryAt(projectTree.selected());

async function remove(entry: FileEntry): Promise<void> {
  const tabs = affectedTabs(entry.path);
  const unsaved = tabs.some(isDirty);
  const question = t(entry.kind === "dir" ? "project.delete.dir" : "project.delete.file", { name: entry.name });
  const ok = await askConfirm({
    title: t("project.delete"),
    message: unsaved ? `${question} ${t("project.delete.unsaved")}` : question,
    confirm: t("project.delete"),
  });
  if (!ok) return;
  try {
    await workspaceApi.delete(entry.path);
  } catch (e) {
    return fail(t("project.deleting"), e);
  }
  for (const t of tabs) closeTab(t.path);
  refreshDirs([parentOf(entry.path)]);
}

async function copy(text: string, what: "path" | "name"): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    showBalloon(t(what === "path" ? "project.copy.failed.path" : "project.copy.failed.name"), t("project.clipboardUnavailable"));
  }
}

const absolute = (path: string) => {
  const root = workspace()?.root ?? "";
  return path ? `${root.replace(/\/$/, "")}/${path}` : root;
};

/**
 * File and Directory, then what other features add for the folder (`project.new` providers
 * get the folder path: file templates such as PHP Class or HTML File).
 */
export function newMenuEntries(dir: string): MenuEntry[] {
  return [
    item(t("project.menu.file"), () => void newEntry(dir, "file"), "NewFile"),
    item(t("project.menu.directory"), () => void newEntry(dir, "dir"), "NewDir"),
    separator,
    ...providedEntries("project.new", dir),
  ];
}

/** Menu of a tree row (`null` = the project root). */
export function fileMenuEntries(entry: FileEntry | null): MenuEntry[] {
  const dir = targetDir(entry);
  const guest = isCollabGuest();
  // A guest changes files only with Files: Edit (the host checks it again).
  const canEdit = guestCanEdit();
  const entries: MenuEntry[] = canEdit
    ? [
        submenu(t("project.menu.new"), newMenuEntries(dir)),
        separator,
      ]
    : [];
  if (entry) {
    entries.push(
      submenu(t("project.menu.copyPathReference"), [
        // The host's path is not shared with guests.
        ...(guest ? [] : [item(t("project.menu.absolutePath"), () => void copy(absolute(entry.path), "path"), "CopyPaths")]),
        item(t("project.menu.pathFromRoot"), () => void copy(entry.path, "path")),
        item(t("project.menu.fileName"), () => void copy(entry.name, "name")),
      ]),
      separator,
    );
  }
  entries.push(item(t("project.menu.findInFiles"), () => openFindInFiles({ directory: dir }), "FindInPath"));
  if (entry && canEdit) {
    // The selected rows (Ctrl+click) move and are deleted together.
    const paths = () => (projectTree.selectedPaths().includes(entry.path) ? projectTree.selectedPaths() : [entry.path]);
    entries.push(
      submenu(t("project.menu.refactor"), [
        item(t("project.menu.rename"), () => void rename(entry), "RenameElement"),
        item(t("project.menu.move"), () => void moveEntries(paths()), "Move"),
        item(t("project.menu.copy"), () => void copyEntries([entry.path]), "CopyElement"),
        item(t("project.menu.safeDelete"), () => void safeDeleteEntries(paths()), "SafeDelete"),
      ]),
      item(t("project.menu.delete"), () => void remove(entry), "$Delete"),
    );
  }
  // Other features (Git) add their submenus next to Reload from Disk, like PHPStorm.
  entries.push(separator, ...providedEntries("project", entry), item(t("project.menu.reload"), () => refreshDirs([dir])));
  // Compare With… / Compare Files below Reload from Disk, like PHPStorm.
  entries.push(separator, ...providedEntries("project.compare", entry));
  return entries;
}

export function showFileMenu(entry: FileEntry | null, x: number, y: number): void {
  showContextMenu(fileMenuEntries(entry), x, y);
}
