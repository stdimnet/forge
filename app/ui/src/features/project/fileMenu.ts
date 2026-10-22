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
import { workspace } from "@/state/workspace";
import { projectTree, refreshDirs, revealPath } from "./ProjectView";
import { openFindInFiles } from "@/features/find/state";

const parentOf = (path: string) => path.split("/").slice(0, -1).join("/");
const join = (dir: string, name: string) => (dir ? `${dir}/${name}` : name);

/** The folder a New action creates in: the folder itself, or the folder of a file. */
const targetDir = (entry: FileEntry | null) => (!entry ? "" : entry.kind === "dir" ? entry.path : parentOf(entry.path));

/** Names that would escape the folder or cannot be a single path segment. */
export function validateName(name: string): string | null {
  if (name === "." || name === "..") return `"${name}" is not a valid name`;
  if (/[\\\0]/.test(name)) return "The name contains characters that are not allowed";
  return null;
}

/** Like `validateName`, but a new entry may name nested folders (`app/Models/Order.php`). */
export function validateNewPath(name: string): string | null {
  if (name.startsWith("/")) return "Use a path relative to the folder";
  const bad = name.split("/").find((s) => s === "" || s === "." || s === ".." || validateName(s));
  return bad === undefined ? null : bad === "" ? "The path has an empty segment" : (validateName(bad) ?? `"${bad}" is not allowed in a path`);
}

function fail(what: string, e: unknown): void {
  showBalloon(`${what} failed`, String(e instanceof Error ? e.message : e));
}

const affectedTabs = (path: string) => editor.tabs.filter((t) => !t.view && (t.path === path || t.path.startsWith(`${path}/`)));

async function newEntry(dir: string, kind: "file" | "dir"): Promise<void> {
  const where = dir || workspace()?.name || "project";
  const name = await askName({
    title: kind === "file" ? "New File" : "New Directory",
    label: `Name (in ${where})`,
    initial: "",
    confirm: "OK",
    validate: validateNewPath,
  });
  if (!name) return;
  const path = join(dir, name);
  try {
    if (kind === "file") await workspaceApi.createFile(path);
    else await workspaceApi.createDir(path);
  } catch (e) {
    return fail(kind === "file" ? "Creating the file" : "Creating the directory", e);
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
    title: `Rename ${entry.kind === "dir" ? "Directory" : "File"}`,
    label: "New name",
    initial: entry.name,
    confirm: "Rename",
    validate: (n) => validateName(n) ?? (n.includes("/") ? "The name cannot contain /" : null),
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
    return fail("Renaming", e);
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
  const what = entry.kind === "dir" ? `the directory "${entry.name}" and everything in it` : `"${entry.name}"`;
  const ok = await askConfirm({
    title: "Delete",
    message: `Delete ${what}? This cannot be undone.${unsaved ? " Unsaved changes in open editors are lost." : ""}`,
    confirm: "Delete",
  });
  if (!ok) return;
  try {
    await workspaceApi.delete(entry.path);
  } catch (e) {
    return fail("Deleting", e);
  }
  for (const t of tabs) closeTab(t.path);
  refreshDirs([parentOf(entry.path)]);
}

async function copy(text: string, what: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    showBalloon(`Cannot copy the ${what}`, "The clipboard is not available.");
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
    item("File", () => void newEntry(dir, "file"), "NewFile"),
    item("Directory", () => void newEntry(dir, "dir"), "NewDir"),
    separator,
    ...providedEntries("project.new", dir),
  ];
}

/** Menu of a tree row (`null` = the project root). */
export function fileMenuEntries(entry: FileEntry | null): MenuEntry[] {
  const dir = targetDir(entry);
  const entries: MenuEntry[] = [
    submenu("New", newMenuEntries(dir)),
    separator,
  ];
  if (entry) {
    entries.push(
      submenu("Copy Path/Reference", [
        item("Absolute Path", () => void copy(absolute(entry.path), "path"), "CopyPaths"),
        item("Path From Project Root", () => void copy(entry.path, "path")),
        item("File Name", () => void copy(entry.name, "name")),
      ]),
      separator,
    );
  }
  entries.push(item("Find in Files…", () => openFindInFiles({ directory: dir }), "FindInPath"));
  if (entry) entries.push(item("Rename…", () => void rename(entry), "RenameElement"), item("Delete…", () => void remove(entry), "$Delete"));
  // Other features (Git) add their submenus next to Reload from Disk, like PHPStorm.
  entries.push(separator, ...providedEntries("project", entry), item("Reload from Disk", () => refreshDirs([dir])));
  // Compare With… / Compare Files below Reload from Disk, like PHPStorm.
  entries.push(separator, ...providedEntries("project.compare", entry));
  return entries;
}

export function showFileMenu(entry: FileEntry | null, x: number, y: number): void {
  showContextMenu(fileMenuEntries(entry), x, y);
}
