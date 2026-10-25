/**
 * Browser-mode Remote Host: the tree of the demo deployment servers, built from the
 * in-memory deployed files of `deploymentMock.ts` (never real files). Mapping highlights,
 * the order of the nodes and the names of New File / Rename come from the Rust code of
 * `forge_deploy::browse` through WebAssembly.
 */
import { registerMock } from "@/ipc";
import { callWasm, forgeWasm } from "@/wasm";
import type { DeployServer } from "@/ipc/deployment";
import type { EditOpened, RemoteListing, RemoteNode, SaveOutcome } from "@/ipc/remoteHost";
import { deployed, pick, type RemoteItem } from "./deploymentMock";

/** Folders created with New Directory (deployed files imply their own folders). */
const emptyDirs = new Map<string, Set<string>>();
/** Edit Remote File: the server copy each open file is based on. */
const edits = new Map<string, string>();

const norm = (p: unknown) =>
  String(p ?? "")
    .split("/")
    .filter((x) => x && x !== ".")
    .join("/");
const parentOf = (p: string) => p.split("/").slice(0, -1).join("/");
const now = () => Math.floor(Date.now() / 1000);

/** A server root for paths: the root path, or the login folder. */
const rootOf = (s: DeployServer) => (s.kind === "local" ? s.rootPath : s.rootPath.replace(/\/+$/, "") || "/home/deploy");

function dirsOf(server: DeployServer): Set<string> {
  let d = emptyDirs.get(server.id);
  if (!d) emptyDirs.set(server.id, (d = new Set()));
  return d;
}

/** Every folder of the server: parents of the deployed files plus the created ones. */
function allDirs(server: DeployServer, files: Map<string, RemoteItem>): Set<string> {
  const out = new Set<string>();
  const add = (p: string) => {
    for (let at = p; at; at = parentOf(at)) out.add(at);
  };
  for (const f of files.keys()) add(parentOf(f));
  for (const d of dirsOf(server)) add(d);
  return out;
}

async function wasm() {
  return forgeWasm();
}

async function entryPath(path: string, name: string, rename: boolean): Promise<string> {
  const r = await callWasm<{ path?: string; error?: string }>((await wasm()).deploy_entry_path, { path, name, rename });
  if (r.error || r.path === undefined) throw new Error(r.error ?? "Invalid name");
  return r.path;
}

async function nodes(server: DeployServer, entries: { path: string; dir: boolean; size: number; modified: number | null }[]) {
  return callWasm<RemoteNode[]>((await wasm()).deploy_remote_nodes, { server, entries });
}

const exists = (files: Map<string, RemoteItem>, dirs: Set<string>, p: string) => files.has(p) || dirs.has(p);

registerMock("remote_host_list", async ({ serverId, path }): Promise<RemoteListing> => {
  const server = pick(serverId);
  const files = await deployed(server);
  const dirs = allDirs(server, files);
  const dir = norm(path);
  if (dir && !dirs.has(dir)) throw new Error(`Cannot list ${rootOf(server)}/${dir}: No such file`);
  await new Promise((r) => setTimeout(r, 120));
  const entries = [
    ...[...dirs].filter((d) => parentOf(d) === dir).map((d) => ({ path: d, dir: true, size: 0, modified: now() - 86_400 })),
    ...[...files.entries()]
      .filter(([p]) => parentOf(p) === dir)
      .map(([p, f]) => ({ path: p, dir: false, size: new TextEncoder().encode(f.text).length, modified: f.modified })),
  ];
  const [rootNode] = await nodes(server, [{ path: "", dir: true, size: 0, modified: null }]);
  return { root: rootOf(server), rootNode: { ...rootNode, name: rootOf(server) }, nodes: await nodes(server, entries) };
});

registerMock("remote_host_create", async ({ serverId, dir, name, folder }): Promise<RemoteNode> => {
  const server = pick(serverId);
  const files = await deployed(server);
  const path = await entryPath(norm(dir), String(name), false);
  if (exists(files, allDirs(server, files), path)) throw new Error(`${rootOf(server)}/${path} already exists`);
  if (folder) dirsOf(server).add(path);
  else files.set(path, { text: "", modified: now() });
  const [node] = await nodes(server, [{ path, dir: !!folder, size: 0, modified: now() }]);
  return node;
});

registerMock("remote_host_rename", async ({ serverId, path, newName }): Promise<string> => {
  const server = pick(serverId);
  const files = await deployed(server);
  const from = norm(path);
  const to = await entryPath(from, String(newName), true);
  if (to === from) return to;
  const dirs = allDirs(server, files);
  if (!exists(files, dirs, from)) throw new Error(`${rootOf(server)}/${from} does not exist`);
  if (exists(files, dirs, to)) throw new Error(`${rootOf(server)}/${to} already exists`);
  const move = (p: string) => (p === from ? to : `${to}${p.slice(from.length)}`);
  for (const [p, f] of [...files.entries()]) {
    if (p === from || p.startsWith(`${from}/`)) {
      files.delete(p);
      files.set(move(p), f);
    }
  }
  const created = dirsOf(server);
  for (const d of [...created]) {
    if (d === from || d.startsWith(`${from}/`)) {
      created.delete(d);
      created.add(move(d));
    }
  }
  return to;
});

registerMock("remote_host_delete", async ({ serverId, paths }): Promise<number> => {
  const server = pick(serverId);
  const files = await deployed(server);
  let deleted = 0;
  for (const raw of paths as string[]) {
    const p = norm(raw);
    if (!p) throw new Error("The root folder is never deleted");
    const within = (x: string) => x === p || x.startsWith(`${p}/`);
    const before = files.size + dirsOf(server).size;
    const wasDir = allDirs(server, files).has(p);
    for (const f of [...files.keys()]) if (within(f)) files.delete(f);
    for (const d of [...dirsOf(server)]) if (within(d)) dirsOf(server).delete(d);
    if (wasDir || files.size + dirsOf(server).size !== before) deleted++;
  }
  return deleted;
});

registerMock("remote_host_edit", async ({ serverId, path }): Promise<EditOpened> => {
  const server = pick(serverId);
  const files = await deployed(server);
  const p = norm(path);
  const file = files.get(p);
  if (!file) throw new Error(`${rootOf(server)}/${p} does not exist`);
  edits.set(`${server.id}\0${p}`, file.text);
  return { text: file.text, temp: `/home/dev/.cache/forge/remote-edit/${server.id}/${p}`, remote: `${rootOf(server)}/${p}` };
});

registerMock("remote_host_save_edit", async ({ serverId, path, text, force }): Promise<SaveOutcome> => {
  const server = pick(serverId);
  const files = await deployed(server);
  const p = norm(path);
  const key = `${server.id}\0${p}`;
  if (!edits.has(key)) throw new Error(`${p} is not open with Edit Remote File any more`);
  const current = files.get(p);
  if (!force && (!current || current.text !== edits.get(key))) return { status: "conflict", deleted: !current };
  files.set(p, { text: String(text), modified: now() });
  edits.set(key, String(text));
  return { status: "uploaded" };
});

registerMock("remote_host_close_edit", ({ serverId, path }) => {
  edits.delete(`${serverId}\0${norm(path)}`);
});

registerMock("remote_host_disconnect", () => undefined);
