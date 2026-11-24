/**
 * Cut, Copy and Paste of files and folders in the Project view (Ctrl+X / Ctrl+C / Ctrl+V and
 * the context menu), like PHPStorm. Copy and Cut keep the selected entries on a file
 * clipboard and put their paths on the system clipboard as text. Paste into the folder they
 * came from opens the Copy dialog with a new name (a duplicate); into another folder it copies
 * them there, or moves them after Cut, through the Copy and Move refactorings.
 *
 * The clipboard keys are native actions (`$Cut`, `$Copy`, `$Paste`), so the keymap leaves
 * them to the browser: the document's clipboard events run them while the tree has the focus.
 */
import { createSignal } from "solid-js";
import { workspaceApi } from "@/ipc/workspace";
import { showBalloon } from "@/keymap";
import { isCollabGuest, workspace } from "@/state/workspace";
import { guestCanEdit } from "@/features/code-with-me/guest";
import { writeClipboard } from "@/features/clipboard/system";
import { t } from "@/i18n";
import { copyEntries, moveEntries } from "@/features/codeintel/moveRefactorings";
import { projectTree } from "./ProjectView";

export interface FileClipboard {
  paths: string[];
  cut: boolean;
  /** What went to the system clipboard, to notice that something else was copied since. */
  text: string;
}

const [fileClipboard, setFileClipboard] = createSignal<FileClipboard | null>(null);
export { fileClipboard };

/** Forget the files on the clipboard. */
export const resetFileClipboard = () => setFileClipboard(null);

/** Whether the system clipboard still holds what Copy put there (Windows reads it back with CRLF). */
export const sameClipboardText = (read: string, written: string) => read.replace(/\r\n/g, "\n") === written.replace(/\r\n/g, "\n");

const parentOf = (path: string) => (path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "");
const baseName = (path: string) => path.split("/").pop() ?? path;

export type PastePlan =
  | { kind: "none" }
  /** One entry pasted into its own folder: the Copy dialog asks for a new name. */
  | { kind: "duplicate"; path: string }
  | { kind: "copy"; paths: string[]; dir: string }
  | { kind: "move"; paths: string[]; dir: string }
  /** A folder pasted into itself or a folder inside it. */
  | { kind: "intoItself"; path: string };

/** What pasting `paths` into the folder `dir` ("" is the project root) does. */
export function pastePlan(paths: string[], cut: boolean, dir: string): PastePlan {
  if (!paths.length) return { kind: "none" };
  // A copied folder that is still selected pastes next to itself.
  if (paths.includes(dir)) dir = parentOf(dir);
  const inside = paths.find((p) => dir === p || dir.startsWith(`${p}/`));
  if (inside) return { kind: "intoItself", path: inside };
  if (cut) {
    const moving = paths.filter((p) => parentOf(p) !== dir);
    return moving.length ? { kind: "move", paths: moving, dir } : { kind: "none" };
  }
  if (paths.length === 1 && parentOf(paths[0]) === dir) return { kind: "duplicate", path: paths[0] };
  return { kind: "copy", paths, dir };
}

/** The text Copy puts on the system clipboard: one path per line (project-relative for a guest). */
export function clipboardText(paths: string[]): string {
  // The host's paths are not shared with guests.
  if (isCollabGuest()) return paths.join("\n");
  const root = (workspace()?.root ?? "").replace(/\/$/, "");
  return paths.map((p) => (p ? `${root}/${p}` : root)).join("\n");
}

/** The selected rows the clipboard actions work on (no virtual rows such as libraries). */
const selection = () => projectTree.selectedPaths().filter((p) => p && !p.includes("://"));

/** Whether the Project tree (not the Structure or Remote Host tree) has the keyboard focus. */
export const projectViewFocused = () => !!document.activeElement?.closest("[data-project-view]");

function put(paths: string[], cut: boolean): string {
  const text = clipboardText(paths);
  setFileClipboard({ paths: [...paths], cut: cut && guestCanEdit(), text });
  return text;
}

/** Copy (`$Copy`) or Cut (`$Cut`) entries from the menu; the system clipboard gets their paths. */
export async function copyFiles(paths: string[], cut: boolean): Promise<void> {
  if (!paths.length) return;
  await writeClipboard(put(paths, cut));
}

export const copySelectedFiles = (cut: boolean) => copyFiles(selection(), cut);

/** Whether Paste has something to paste. */
export const canPasteFiles = () => !!fileClipboard()?.paths.length && guestCanEdit();

/** The folder Paste goes into: the selected folder, the folder of the selected file, else the root. */
async function selectedDir(): Promise<string> {
  const path = projectTree.selected();
  if (!path || path.includes("://")) return "";
  const entry = await workspaceApi
    .listDir(parentOf(path))
    .then((entries) => entries.find((e) => e.path === path) ?? null)
    .catch(() => null);
  return entry?.kind === "dir" ? path : parentOf(path);
}

/** Paste (`$Paste`) the file clipboard into `dir`, or into the folder of the selection. */
export async function pasteFiles(dir?: string): Promise<void> {
  const clip = fileClipboard();
  if (!clip || !guestCanEdit()) return;
  const plan = pastePlan(clip.paths, clip.cut, dir ?? (await selectedDir()));
  switch (plan.kind) {
    case "none":
      return;
    case "intoItself":
      showBalloon(t("project.failed", { what: t("project.pasting") }), t("project.paste.intoItself", { name: baseName(plan.path) }), 6000);
      return;
    case "duplicate":
      await copyEntries([plan.path]);
      return;
    case "copy":
      await copyEntries(plan.paths, plan.dir);
      return;
    case "move":
      // Moved entries are gone from where they were cut, like in PHPStorm.
      if (await moveEntries(plan.paths, plan.dir)) setFileClipboard(null);
  }
}

if (typeof document !== "undefined") {
  // Text selected inside the tree is copied as text; a selection left elsewhere (an editor
  // that lost the focus) does not count.
  const inTree = () => {
    const tree = document.activeElement?.closest("[data-project-view]");
    const sel = document.getSelection();
    return !!tree && !(sel && !sel.isCollapsed && tree.contains(sel.anchorNode));
  };
  // WebKit and Blink fire Copy/Cut/Paste on content that is not editable only when its
  // `before…` event is cancelled.
  for (const type of ["beforecopy", "beforecut", "beforepaste"]) {
    document.addEventListener(type, (e) => inTree() && e.preventDefault(), true);
  }
  const onCopy = (cut: boolean) => (e: ClipboardEvent) => {
    if (!inTree()) {
      // Copying something else replaces the files on the clipboard.
      setFileClipboard(null);
      return;
    }
    const paths = selection();
    if (!paths.length) return;
    e.preventDefault();
    e.stopPropagation();
    const text = put(paths, cut);
    if (e.clipboardData) e.clipboardData.setData("text/plain", text);
    else void writeClipboard(text);
  };
  document.addEventListener("copy", onCopy(false), true);
  document.addEventListener("cut", onCopy(true), true);
  document.addEventListener(
    "paste",
    (e) => {
      if (!inTree()) return;
      e.preventDefault();
      e.stopPropagation();
      const clip = fileClipboard();
      const text = e.clipboardData?.getData("text/plain");
      // Copied outside Forge since: the files are no longer on the clipboard.
      if (clip && text && !sameClipboardText(text, clip.text)) {
        setFileClipboard(null);
        return;
      }
      void pasteFiles();
    },
    true,
  );
}
