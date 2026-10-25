/**
 * Browser-mode Deployment: three demo servers with their files in memory (never on disk).
 * Validation, path mappings, exclusions and the Sync with Deployed comparison run the Rust
 * code of `forge-deploy` through WebAssembly; transfers stream `deploy://transfer` events
 * with a short delay per file so the File Transfer tool window shows progress. Downloads
 * write into the in-memory workspace mock only.
 */
import { emitMock, invoke, registerMock } from "@/ipc";
import { callWasm, forgeWasm } from "@/wasm";
import type { DirCompare } from "@/features/compare/api";
import type {
  DeployServer,
  Direction,
  FileResult,
  RemoteFile,
  ServerView,
  Summary,
  SyncCompare,
  TestResult,
  TransferEvent,
} from "@/ipc/deployment";
import { TRANSFER_EVENT } from "@/ipc/deployment";
import { mockFiles } from "./workspaceMock";
import { mockSettingValue } from "./settingsMock";
import { mockSafeMode } from "./trustMock";

interface RemoteItem {
  text: string;
  /** Unix seconds. */
  modified: number;
}

const DEFAULT_MAPPINGS = [{ local: "", deployment: "/", web: "" }];
const servers: DeployServer[] = [
  {
    id: "shop-production",
    name: "shop-production",
    kind: "sftp",
    sshConfig: "shop-key",
    host: "",
    port: null,
    user: "",
    anonymous: false,
    passive: true,
    rootPath: "/var/www/shop",
    webUrl: "https://shop.example.com",
    mappings: [
      { local: "", deployment: "/", web: "" },
      { local: "public", deployment: "/public", web: "/" },
    ],
    excluded: [
      { side: "local", path: "storage" },
      { side: "local", path: "node_modules" },
      { side: "deployment", path: "/public/uploads" },
    ],
  },
  {
    id: "shop-staging",
    name: "shop-staging",
    kind: "ftps",
    sshConfig: "",
    host: "ftp.staging.shop.internal",
    port: null,
    user: "deploy",
    anonymous: false,
    passive: true,
    rootPath: "/htdocs",
    webUrl: "https://staging.shop.example.com",
    mappings: DEFAULT_MAPPINGS,
    excluded: [{ side: "local", path: "storage" }],
  },
  {
    id: "local-build",
    name: "Local Apache",
    kind: "local",
    sshConfig: "",
    host: "",
    port: null,
    user: "",
    anonymous: false,
    passive: true,
    rootPath: "/srv/http/shop",
    webUrl: "http://localhost:8080",
    mappings: DEFAULT_MAPPINGS,
    excluded: [],
  },
];
let defaultServer: string | null = "shop-production";
const secrets = new Set(["shop-staging"]);

const NOW = Math.floor(Date.now() / 1000);
/** Deployed files per server, keyed by deployment path (relative to the root path). */
const remote = new Map<string, Map<string, RemoteItem>>();

async function wasm() {
  return forgeWasm();
}

async function resolve(server: DeployServer, paths: string[]) {
  const w = await wasm();
  return callWasm<{ local: string; status: "mapped" | "notMapped" | "excluded"; deployment?: string; display?: string }[]>(
    w.deploy_resolve,
    { server, paths, excludeNames: String(mockSettingValue("deployment.excludeNames") ?? "") },
  );
}

/** The demo deployment: production lags one change behind and has a maintenance page. */
async function deployed(server: DeployServer): Promise<Map<string, RemoteItem>> {
  let files = remote.get(server.id);
  if (files) return files;
  files = new Map();
  remote.set(server.id, files);
  if (server.kind === "local") return files;
  const paths = Object.keys(mockFiles);
  for (const r of await resolve(server, paths)) {
    if (r.status !== "mapped" || r.deployment === undefined) continue;
    files.set(r.deployment, { text: mockFiles[r.local], modified: NOW - 86_400 });
  }
  if (server.id === "shop-production") {
    const user = [...files.keys()].find((p) => p.endsWith("Models/User.php"));
    if (user) files.set(user, { text: files.get(user)!.text.replace(/\n\s*\/\*\*[^]*?\*\/\n/, "\n"), modified: NOW - 3_600 });
    files.set("public/maintenance.html", { text: "<h1>Back soon</h1>\n", modified: NOW - 7_200 });
    const env = [...files.keys()].find((p) => p === ".env.example");
    if (env) files.delete(env);
  }
  return files;
}

const view = (s: DeployServer): ServerView => ({
  ...structuredClone(s),
  label: s.name || s.host || s.rootPath,
  hasSecret: s.kind === "sftp" ? s.sshConfig === "staging" : secrets.has(s.id),
});

const pick = (id: unknown): DeployServer => {
  const server = servers.find((s) => s.id === (id ?? defaultServer));
  if (!server) throw new Error(id ? `The deployment server ${id} does not exist` : "No default deployment server. Add one in Settings | Build, Execution, Deployment | Deployment.");
  return server;
};

function newId(name: string): string {
  const base = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "server";
  let id = base;
  for (let n = 2; servers.some((s) => s.id === id); n++) id = `${base}-${n}`;
  return id;
}

registerMock("deploy_config", () => ({
  defaultServer,
  servers: servers.map(view),
  available: true,
}));

registerMock("deploy_server_save", ({ server, secret }) => {
  const s = structuredClone(server as DeployServer);
  if (!s.id) s.id = newId(s.name);
  const at = servers.findIndex((x) => x.id === s.id);
  if (at >= 0) servers[at] = s;
  else servers.push(s);
  if (!defaultServer) defaultServer = s.id;
  if (typeof secret === "string") (secret ? secrets.add(s.id) : secrets.delete(s.id));
  remote.delete(s.id);
  return view(s);
});

registerMock("deploy_server_delete", ({ id }) => {
  const at = servers.findIndex((s) => s.id === id);
  if (at >= 0) servers.splice(at, 1);
  if (defaultServer === id) defaultServer = servers[0]?.id ?? null;
});

registerMock("deploy_set_default", ({ id }) => {
  defaultServer = (id as string | null) ?? null;
});

registerMock("deploy_validate", async ({ server }) => callWasm<string[]>((await wasm()).deploy_validate, server));

registerMock("deploy_test", async ({ server }): Promise<TestResult> => {
  const s = server as DeployServer;
  await new Promise((r) => setTimeout(r, 400));
  if (s.kind === "local") return { ok: true, message: `Connected to ${s.name || s.rootPath}`, root: s.rootPath };
  if (s.host.includes("fail") || (s.kind === "sftp" && !s.sshConfig)) {
    return { ok: false, message: `Cannot connect to ${s.host || "the host"}:${s.port ?? 21}: Connection refused (os error 111)` };
  }
  return { ok: true, message: `Connected to ${s.name || s.host}`, root: s.rootPath || "/home/deploy" };
});

const running = new Map<string, { cancelled: boolean }>();
const pause = () => new Promise((r) => setTimeout(r, 90));
const emit = (e: TransferEvent) => emitMock(TRANSFER_EVENT, e);

/** Project files below the given paths. */
function filesBelow(paths: string[]): string[] {
  const all = Object.keys(mockFiles);
  const out = new Set<string>();
  for (const p of paths) {
    const base = p.replace(/^\/+|\/+$/g, "");
    for (const f of all) if (!base || f === base || f.startsWith(`${base}/`)) out.add(f);
  }
  return [...out].sort();
}

function summarize(results: FileResult[], cancelled: boolean): Summary {
  const count = (s: FileResult["status"]) => results.filter((r) => r.status === s).length;
  return {
    results,
    transferred: count("transferred"),
    skipped: results.length - count("transferred") - count("deleted") - count("failed"),
    deleted: count("deleted"),
    failed: count("failed"),
    bytes: results.reduce((n, r) => n + (r.status === "transferred" ? r.bytes : 0), 0),
    newerOnTarget: results.filter((r) => r.status === "newerOnTarget").map((r) => r.local),
    cancelled,
  };
}

async function transfer(id: string, server: DeployServer, direction: Direction, paths: string[], overwriteNewer: boolean, automatic: boolean) {
  const files = await deployed(server);
  const state = { cancelled: false };
  running.set(id, state);
  const verb = direction === "upload" ? "Upload to" : "Download from";
  emit({ kind: "started", id, server: server.name, operation: `${verb} ${server.name}`, paths, automatic });
  const results: FileResult[] = [];
  const locals = direction === "upload" ? filesBelow(paths) : await localsOnServer(server, paths, files);
  for (const r of await resolve(server, locals)) {
    if (state.cancelled) break;
    let result: FileResult;
    if (r.status !== "mapped" || r.deployment === undefined) {
      // Excluded names stay out of the log, like the backend's folder walk.
      if (r.status === "excluded" && !paths.includes(r.local)) continue;
      result = { local: r.local, remote: "", dir: false, status: r.status === "excluded" ? "excluded" : "notMapped", bytes: 0 };
    } else {
      const there = files.get(r.deployment);
      const text = direction === "upload" ? mockFiles[r.local] : there?.text;
      const target = direction === "upload" ? there?.text : mockFiles[r.local];
      if (text === undefined) {
        result = { local: r.local, remote: r.display ?? "", dir: false, status: "failed", bytes: 0, message: `${r.display} does not exist` };
      } else if (target === text && !mockSettingValue("deployment.overwriteUpToDate")) {
        result = { local: r.local, remote: r.display ?? "", dir: false, status: "upToDate", bytes: 0 };
      } else if (direction === "upload" && there && there.modified > NOW - 4_000 && !overwriteNewer && mockSettingValue("deployment.warnOverNewer") !== "no") {
        result = { local: r.local, remote: r.display ?? "", dir: false, status: "newerOnTarget", bytes: 0 };
      } else {
        await pause();
        if (direction === "upload") files.set(r.deployment, { text, modified: Math.floor(Date.now() / 1000) });
        else await invoke("fs_write_file", { path: r.local, contents: text });
        result = { local: r.local, remote: r.display ?? "", dir: false, status: "transferred", bytes: new TextEncoder().encode(text).length };
      }
    }
    results.push(result);
    emit({ kind: "file", id, file: result });
  }
  running.delete(id);
  const summary = summarize(results, state.cancelled);
  emit({ kind: "finished", id, summary, error: null });
  return summary;
}

/** Downloads: the project paths of the deployed files below the requested paths. */
async function localsOnServer(server: DeployServer, paths: string[], files: Map<string, RemoteItem>): Promise<string[]> {
  const mapped = await resolve(server, paths);
  const out = new Set<string>();
  for (const m of mapped) {
    if (m.status !== "mapped" || m.deployment === undefined) {
      out.add(m.local);
      continue;
    }
    const base = m.deployment;
    for (const p of files.keys()) {
      if (base && p !== base && !p.startsWith(`${base}/`)) continue;
      const rest = base ? p.slice(base.length).replace(/^\//, "") : p;
      out.add(rest ? (m.local ? `${m.local}/${rest}` : rest) : m.local);
    }
  }
  // Keep only paths that map back to where they were found (nested mappings).
  const back = await resolve(server, [...out]);
  return back.filter((b) => b.status !== "mapped" || files.has(b.deployment!)).map((b) => b.local);
}

registerMock("deploy_transfer", ({ id, serverId, direction, paths, overwriteNewer }) =>
  transfer(String(id), pick(serverId), direction as Direction, paths as string[], !!overwriteNewer, false),
);

registerMock("deploy_delete", async ({ id, serverId, paths }) => {
  const server = pick(serverId);
  const files = await deployed(server);
  emit({ kind: "started", id: String(id), server: server.name, operation: `Delete from ${server.name}`, paths: paths as string[], automatic: false });
  const results: FileResult[] = [];
  for (const r of await resolve(server, paths as string[])) {
    let result: FileResult = { local: r.local, remote: "", dir: false, status: r.status === "mapped" ? "deleted" : r.status, bytes: 0 };
    if (r.status === "mapped" && r.deployment !== undefined) {
      if (!r.deployment) result = { ...result, status: "failed", message: "The root path itself is never deleted" };
      else {
        for (const p of [...files.keys()]) if (p === r.deployment || p.startsWith(`${r.deployment}/`)) files.delete(p);
        result = { ...result, remote: r.display ?? "" };
      }
    }
    results.push(result);
    emit({ kind: "file", id: String(id), file: result });
  }
  const summary = summarize(results, false);
  emit({ kind: "finished", id: String(id), summary, error: null });
  return summary;
});

registerMock("deploy_cancel", ({ id }) => {
  const r = running.get(String(id));
  if (r) r.cancelled = true;
});

registerMock("deploy_auto_upload", async ({ id, paths, explicit }) => {
  const mode = String(mockSettingValue("deployment.autoUpload") ?? "never");
  const uploads = mode === "always" || (mode === "explicit" && !!explicit);
  if (!uploads || mockSafeMode() || !defaultServer) return null;
  return transfer(String(id), pick(null), "upload", paths as string[], false, true);
});

registerMock("deploy_sync_compare", async ({ serverId, path }): Promise<SyncCompare> => {
  const server = pick(serverId);
  const files = await deployed(server);
  const dir = String(path ?? "").replace(/^\/+|\/+$/g, "");
  const [root] = await resolve(server, [dir]);
  if (root.status !== "mapped" || root.deployment === undefined) throw new Error(`${dir || "The project root"} is not mapped to ${server.name}`);
  const rel = (p: string, base: string) => (base ? p.slice(base.length + 1) : p);
  const localFiles = filesBelow([dir]);
  const keep = await resolve(server, localFiles);
  const local = keep
    .filter((k) => k.status === "mapped")
    .map((k) => ({ path: rel(k.local, dir), dir: false, text: mockFiles[k.local], modified: NOW - 600 }));
  const base = root.deployment;
  const remoteItems = [...files.entries()]
    .filter(([p]) => !base || p.startsWith(`${base}/`))
    .map(([p, f]) => ({ path: rel(p, base), dir: false, text: f.text, modified: f.modified }));
  const by = String(mockSettingValue("deployment.compareBy") ?? "content");
  const compare = await callWasm<DirCompare>((await wasm()).deploy_compare, { local, remote: remoteItems, by });
  return { local: dir, remote: root.display ?? "", compare };
});

registerMock("deploy_read_remote", async ({ serverId, path }): Promise<RemoteFile> => {
  const server = pick(serverId);
  const files = await deployed(server);
  const [r] = await resolve(server, [String(path)]);
  if (r.status === "notMapped") throw new Error(`${path} is not mapped to ${server.name}`);
  if (r.status === "excluded") throw new Error(`${path} is excluded from ${server.name}`);
  return { text: files.get(r.deployment!)?.text ?? null, binary: false, remote: r.display ?? "", server: server.name };
});
