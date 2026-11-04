/**
 * Remote Host tool window state: the selected deployment server, the loaded folders of its
 * lazy tree, expansion and selection, and the actions of the context menu. Listing and file
 * operations happen in Rust (`forge_deploy::remote_host`, `remote_host_*` commands);
 * transfers and comparisons go through the deployment actions with the node's mapped
 * project path.
 */
import { createSignal } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { remoteHostApi, type RemoteNode, type SaveOutcome } from "@/ipc/remoteHost";
import type { ServerView } from "@/ipc/deployment";
import { showBalloon } from "@/keymap";
import { askChoice, askConfirm, askName } from "@/features/contextMenu/dialogs";
import { compareWithDeployed, config, defaultServer, transfer } from "@/features/deployment/store";
import { openFile } from "@/state/editor";
import { t as tr } from "@/i18n";
import { writeClipboard } from "@/features/clipboard/system";

export const ROOT = "";
/** Editor paths of Edit Remote File tabs: `remote-host://<server id>/<path>`. */
export const EDIT_PREFIX = "remote-host://";

const [serverId, setServerIdSignal] = createSignal<string | null>(null);
export { serverId };

/** The server the window shows: the chosen one, else the project's default server. */
export const currentServer = (): ServerView | undefined => {
  const servers = config()?.servers ?? [];
  return servers.find((s) => s.id === serverId()) ?? defaultServer() ?? servers[0];
};

interface TreeState {
  /** Loaded folders: their children. */
  children: Record<string, RemoteNode[]>;
  /** The server's absolute root folder. */
  root: string;
  rootNode: RemoteNode | null;
  expanded: Record<string, boolean>;
  loading: Record<string, boolean>;
  errors: Record<string, string>;
  selected: string | null;
}

const empty = (): TreeState => ({ children: {}, root: "", rootNode: null, expanded: { [ROOT]: true }, loading: {}, errors: {}, selected: null });

const [tree, setTree] = createStore<TreeState>(empty());
export { tree };

const message = (e: unknown) => String(e instanceof Error ? e.message : e);

export function selectServer(id: string): void {
  if (id === currentServer()?.id && tree.children[ROOT]) return;
  const previous = currentServer()?.id;
  setServerIdSignal(id);
  setTree(empty());
  if (previous && previous !== id) void remoteHostApi.disconnect(previous).catch(() => undefined);
  void load(ROOT);
}

/** List a folder (again). */
export async function load(path: string): Promise<void> {
  const server = currentServer();
  if (!server) return;
  setTree("loading", path, true);
  setTree("errors", produce((e) => void delete e[path]));
  try {
    const listing = await remoteHostApi.list(server.id, path);
    if (currentServer()?.id !== server.id) return;
    setTree(
      produce((t) => {
        t.children[path] = listing.nodes;
        t.root = listing.root;
        if (path === ROOT) t.rootNode = listing.rootNode;
      }),
    );
  } catch (e) {
    setTree("errors", path, message(e));
  } finally {
    setTree("loading", produce((l) => void delete l[path]));
  }
}

export async function toggle(node: RemoteNode): Promise<void> {
  if (!node.dir) return;
  const open = !tree.expanded[node.path];
  setTree("expanded", node.path, open);
  if (open && !tree.children[node.path]) await load(node.path);
}

export const select = (path: string | null) => setTree("selected", path);

/** Refresh: the selected folder (or the folder of the selected file), or the root. */
export async function refresh(path?: string | null): Promise<void> {
  const target = path ?? folderOf(tree.selected);
  // Expanded folders below the refreshed one load again too.
  const below = Object.keys(tree.children).filter((p) => p !== target && (target === ROOT || p.startsWith(`${target}/`)));
  setTree(
    "children",
    produce((c) => {
      for (const p of below) delete c[p];
    }),
  );
  await load(target);
  await Promise.all(below.filter((p) => tree.expanded[p]).map((p) => load(p)));
}

export function collapseAll(): void {
  setTree("expanded", { [ROOT]: true });
}

const parentOf = (path: string) => path.split("/").slice(0, -1).join("/");

/** The node with `path` among the loaded folders. */
export function nodeAt(path: string | null): RemoteNode | undefined {
  if (path === null) return undefined;
  if (path === ROOT) return tree.rootNode ?? undefined;
  return tree.children[parentOf(path)]?.find((n) => n.path === path);
}

/** A folder itself, or the folder a file is in. */
function folderOf(path: string | null): string {
  if (!path) return ROOT;
  const node = nodeAt(path);
  return node?.dir ? path : parentOf(path);
}

/** Copy Path: the absolute path on the server. */
export function absolutePath(path: string): string {
  const root = tree.root.replace(/\/+$/, "");
  return path ? `${root}/${path}` : root || "/";
}

// ---- Context menu actions -----------------------------------------------------------------------

function withServer(run: (s: ServerView) => Promise<void>): Promise<void> {
  const s = currentServer();
  if (!s) {
    showBalloon(tr("tools.remoteHost.noDeploymentServer"), tr("tools.remoteHost.addAServerInSettings"));
    return Promise.resolve();
  }
  return run(s);
}

export const editPath = (server: string, path: string) => `${EDIT_PREFIX}${server}/${path}`;

/** `remote-host://<server>/<path>` → its parts. */
export function parseEditPath(path: string): { server: string; path: string } | null {
  if (!path.startsWith(EDIT_PREFIX)) return null;
  const rest = path.slice(EDIT_PREFIX.length);
  const at = rest.indexOf("/");
  return at < 0 ? null : { server: rest.slice(0, at), path: rest.slice(at + 1) };
}

/** Edit Remote File: a temporary copy in the editor, uploaded back on save. */
export const editRemoteFile = (node: RemoteNode) =>
  withServer(async (s) => {
    try {
      await openFile(editPath(s.id, node.path));
    } catch (e) {
      showBalloon(tr("tools.remoteHost.cannotEdit", { name: node.name }), message(e), 5000);
    }
  });

/** Saving an Edit Remote File tab: upload, asking when the server copy changed meanwhile. */
export async function saveRemoteEdit(editorPath: string, text: string): Promise<void> {
  const target = parseEditPath(editorPath);
  if (!target) throw new Error(`${editorPath} is not a remote file`);
  const server = config()?.servers.find((s) => s.id === target.server);
  const name = server?.name ?? target.server;
  let outcome: SaveOutcome = await remoteHostApi.saveEdit(target.server, target.path, text);
  if (outcome.status === "conflict") {
    const choice = await askChoice({
      title: "File Changed on Server",
      message: outcome.deleted
        ? `${target.path} was deleted on ${name} after it was opened. Upload your version anyway?`
        : `${target.path} was changed on ${name} after it was opened. Overwrite the server version with yours?`,
      choices: [
        { id: "cancel", label: "Cancel" },
        { id: "overwrite", label: outcome.deleted ? "Upload" : "Overwrite" },
      ],
    });
    if (choice !== "overwrite") throw new Error(`${target.path} was not uploaded: the server copy changed`);
    outcome = await remoteHostApi.saveEdit(target.server, target.path, text, true);
  }
  showBalloon(tr("tools.remoteHost.uploadedTo", { value: target.path.split("/").pop(), name }));
  if (currentServer()?.id === target.server) void load(parentOf(target.path));
}

export const downloadFromHere = (node: RemoteNode) =>
  withServer(async (s) => {
    if (node.local === null) return void showBalloon(tr("tools.remoteHost.isNotMappedToA", { name: node.name }), tr("tools.remoteHost.addAMappingInThe"));
    await transfer(s, "download", [node.local]);
  });

export const uploadToHere = (node: RemoteNode) =>
  withServer(async (s) => {
    if (node.local === null) return void showBalloon(tr("tools.remoteHost.isNotMappedToA", { name: node.name }), tr("tools.remoteHost.addAMappingInThe"));
    await transfer(s, "upload", [node.local]);
    await refresh(node.dir ? node.path : parentOf(node.path));
  });

export const compareWithLocal = (node: RemoteNode) =>
  withServer(async (s) => {
    if (node.local === null) return void showBalloon(tr("tools.remoteHost.isNotMappedToA", { name: node.name }));
    await compareWithDeployed(node.local, s);
  });

/** New File / New Directory in the selected folder (or the folder of the selected file). */
export const createEntry = (folder: boolean, at: string | null = tree.selected) =>
  withServer(async (s) => {
    const dir = folderOf(at);
    const name = await askName({
      title: folder ? "New Directory" : "New File",
      label: folder ? "Enter new directory name:" : "Enter new file name:",
      initial: "",
      confirm: "OK",
    });
    if (!name) return;
    try {
      const node = await remoteHostApi.create(s.id, dir, name, folder);
      setTree("expanded", dir, true);
      await load(dir);
      select(node.path);
      if (!folder) await editRemoteFile(node);
    } catch (e) {
      showBalloon(tr("tools.remoteHost.cannotCreate", { name }), message(e), 5000);
    }
  });

export const renameEntry = (node: RemoteNode) =>
  withServer(async (s) => {
    const name = await askName({ title: "Rename", label: `Rename ${node.dir ? "directory" : "file"} to:`, initial: node.name, confirm: "Rename" });
    if (!name || name === node.name) return;
    try {
      const path = await remoteHostApi.rename(s.id, node.path, name);
      await load(parentOf(node.path));
      select(path);
    } catch (e) {
      showBalloon(tr("tools.remoteHost.cannotRename", { name: node.name }), message(e), 5000);
    }
  });

export const deleteEntry = (node: RemoteNode) =>
  withServer(async (s) => {
    const ok = await askConfirm({
      title: "Delete",
      message: `Delete ${node.dir ? "the directory" : ""} ${absolutePath(node.path)} on ${s.name}${node.dir ? " with everything in it" : ""}? This cannot be undone.`,
      confirm: "Delete",
    });
    if (!ok) return;
    try {
      await remoteHostApi.delete(s.id, [node.path]);
      if (tree.selected === node.path) select(null);
      await load(parentOf(node.path));
    } catch (e) {
      showBalloon(tr("tools.remoteHost.cannotDelete", { name: node.name }), message(e), 5000);
    }
  });

export async function copyPath(node: RemoteNode): Promise<void> {
  const text = absolutePath(node.path);
  if (await writeClipboard(text)) showBalloon(tr("tools.remoteHost.pathCopied"), text);
}
