/**
 * Browser-mode remote development: a fake host `deploy@shop.example.com` whose project
 * `/var/www/shop` is the demo Laravel project. `?project=remote` opens it right away.
 * Host key checks, `~/.ssh/config` parsing and configuration checks run the real Rust code
 * (WebAssembly); nothing here touches real files or networks.
 */
import { emitMock, registerMock } from "@/ipc";
import type {
  BrowseEntry,
  BrowseResult,
  ConnectError,
  ConnectOutcome,
  HostKeyInfo,
  RemoteStatus,
  SshConfiguration,
  SshConfigView,
  TestOutcome,
} from "@/ipc/remote";
import type { WorkspaceInfo } from "@/ipc/workspace";
import { callWasm, forgeWasm } from "@/wasm";
import { setSetting } from "./settingsMock";

export const REMOTE_HOST = "shop.example.com";
export const REMOTE_USER = "deploy";
export const REMOTE_PATH = "/var/www/shop";
export const REMOTE_ROOT = `${REMOTE_USER}@${REMOTE_HOST}:${REMOTE_PATH}`;

/** `?project=remote`: the workbench starts with the remote demo project. */
export const isRemoteDemo = (): boolean =>
  typeof location !== "undefined" && new URLSearchParams(location.search).get("project") === "remote";

/** Host keys the fake hosts present (ed25519 blobs). */
const HOST_KEYS: Record<string, string> = {
  "shop.example.com": "AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl",
  "staging.shop.internal": "AAAAC3NzaC1lZDI1NTE5AAAAIBH1bD1mVrOR8vT3+3tW9I+gp5+rtZ5Xj5wIWmhB9i1y",
};
const DEFAULT_KEY = "AAAAC3NzaC1lZDI1NTE5AAAAIK3sX9vQm2b6m0TQh1cYw2Wq7rP3zJdV0nFhE8Lk4aUo";

/** The fake `~/.ssh/known_hosts`: staging is known, the shop host is not yet. */
let knownHosts = `[staging.shop.internal]:2222 ssh-ed25519 ${HOST_KEYS["staging.shop.internal"]}\n`;

const SSH_CONFIG = `Host shop
    HostName shop.example.com
    User deploy
    IdentityFile ~/.ssh/id_ed25519

Host staging
    HostName staging.shop.internal
    Port 2222
    User root

Host *.internal
    ProxyJump bastion
`;

const configs: SshConfigView[] = [
  {
    id: "staging",
    host: "staging.shop.internal",
    port: 2222,
    user: "root",
    auth: "password",
    label: "root@staging.shop.internal:2222",
    hasSecret: true,
  },
  {
    id: "shop-key",
    host: "shop.example.com",
    port: 22,
    user: "deploy",
    auth: "keyPair",
    keyFile: "~/.ssh/id_ed25519",
    label: "deploy@shop.example.com",
    hasSecret: false,
  },
];

const label = (c: SshConfiguration) => (c.port === 22 ? `${c.user}@${c.host}` : `${c.user}@${c.host}:${c.port}`);

/** Resolve a `Host` alias like the backend does with `~/.ssh/config`. */
async function resolveHost(c: SshConfiguration): Promise<{ host: string; port: number; user: string }> {
  const hosts = await callWasm<{ alias: string; hostName: string; user?: string; port?: number }[]>(
    (await forgeWasm()).remote_ssh_hosts,
    { text: SSH_CONFIG, home: "/home/dev", user: "dev" },
  );
  const h = hosts.find((x) => x.alias === c.host.trim());
  return {
    host: h?.hostName ?? c.host.trim(),
    port: c.port === 22 && h?.port ? h.port : c.port,
    user: c.user.trim() || h?.user || "dev",
  };
}

type Check = { ok: true; host: string; port: number; user: string } | { ok: false; error: ConnectError };

/** SSH handshake and authentication against the fake hosts. */
async function handshake(c: SshConfiguration, secret?: string, accept?: string): Promise<Check> {
  const problems = await callWasm<string[]>((await forgeWasm()).remote_validate_config, c);
  if (problems.length) return { ok: false, error: { kind: "other", message: problems.join("\n") } };
  const { host, port, user } = await resolveHost(c);
  if (!/^[a-z0-9.-]+$/i.test(host) || host.endsWith(".invalid")) {
    return { ok: false, error: { kind: "network", message: `Cannot connect to ${host}:${port}: no such host` } };
  }
  const key = HOST_KEYS[host] ?? DEFAULT_KEY;
  const verdict = await callWasm<HostKeyInfo & { knownHosts?: string }>((await forgeWasm()).remote_check_host_key, {
    knownHosts,
    host,
    port,
    keyType: "ssh-ed25519",
    key,
    accept,
  });
  if (verdict.knownHosts !== undefined) knownHosts = verdict.knownHosts;
  if (verdict.status !== "trusted") {
    const { knownHosts: _k, ...hostKey } = verdict;
    return { ok: false, error: { kind: "hostKey", hostKey: { ...hostKey, host, port } } };
  }
  const saved = configs.find((x) => x.id === c.id);
  if (c.auth === "password" && !secret && !saved?.hasSecret) {
    return { ok: false, error: { kind: "auth", message: `Authentication failed for ${user}@${host}` } };
  }
  return { ok: true, host, port, user };
}

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

// ---- configurations ----------------------------------------------------------------------

registerMock("remote_ssh_configs", () => structuredClone(configs));
registerMock("remote_ssh_config_save", async ({ config, secret }) => {
  const c = config as SshConfiguration;
  const problems = await callWasm<string[]>((await forgeWasm()).remote_validate_config, c);
  if (problems.length) throw new Error(problems.join("\n"));
  const id = c.id || `${c.user}-${c.host}`.replace(/[^a-z0-9]+/gi, "-").toLowerCase();
  const prev = configs.findIndex((x) => x.id === id);
  const hadSecret = prev >= 0 && configs[prev].hasSecret;
  const view: SshConfigView = {
    ...c,
    id,
    label: label(c),
    hasSecret: !c.dontSaveSecret && (typeof secret === "string" ? secret.length > 0 : hadSecret),
  };
  if (prev >= 0) configs[prev] = view;
  else configs.push(view);
  return structuredClone(view);
});
registerMock("remote_ssh_config_delete", ({ id }) => {
  const i = configs.findIndex((x) => x.id === id);
  if (i >= 0) configs.splice(i, 1);
});
registerMock("remote_validate_config", async ({ config }) =>
  callWasm<string[]>((await forgeWasm()).remote_validate_config, config),
);
registerMock("remote_ssh_hosts", async () =>
  callWasm((await forgeWasm()).remote_ssh_hosts, { text: SSH_CONFIG, home: "/home/dev", user: "dev" }),
);

// ---- connecting ----------------------------------------------------------------------------

registerMock("remote_test_connection", async ({ config, secret, acceptFingerprint }): Promise<TestOutcome> => {
  await wait(250);
  const r = await handshake(config as SshConfiguration, secret as string, acceptFingerprint as string);
  if (!r.ok) return { ok: false, error: r.error };
  return { ok: true, latencyMs: 42, system: "Linux 6.8.0-45-generic x86_64" };
});

let connections = 0;
const connectionHosts = new Map<string, { host: string; port: number; user: string; configId?: string }>();

registerMock("remote_connect", async ({ config, secret, acceptFingerprint }): Promise<ConnectOutcome> => {
  const c = config as SshConfiguration;
  const progress = (message: string) => emitMock("remote://progress", { message });
  progress(`Connecting to ${label(c)}…`);
  await wait(200);
  const r = await handshake(c, secret as string, acceptFingerprint as string);
  if (!r.ok) return { status: "failed", error: r.error };
  progress("Checking the remote host…");
  await wait(250);
  progress("Uploading the Forge agent…");
  await wait(400);
  progress("Starting the Forge agent…");
  await wait(200);
  const id = `conn-${++connections}`;
  connectionHosts.set(id, { host: c.host.trim(), port: r.port, user: r.user, configId: c.id || undefined });
  return {
    status: "connected",
    connectionId: id,
    label: r.port === 22 ? `${r.user}@${c.host.trim()}` : `${r.user}@${c.host.trim()}:${r.port}`,
    home: `/home/${r.user}`,
    hostname: "shop-web-01",
    system: "linux x86_64",
  };
});

registerMock("remote_disconnect", ({ connectionId }) => {
  connectionHosts.delete(String(connectionId));
});

// ---- the remote file system ----------------------------------------------------------------

const dirs = (...names: string[]) => names.map((n) => ({ name: n, dir: true }));
const files = (...names: string[]) => names.map((n) => ({ name: n, dir: false }));

/** Folders of the fake host (the project folder itself lists the demo project). */
const TREE: Record<string, { name: string; dir: boolean; project?: boolean }[]> = {
  "/": dirs("bin", "etc", "home", "opt", "srv", "tmp", "usr", "var"),
  "/home": dirs("deploy"),
  "/home/deploy": [...dirs(".cache", ".ssh", "apps", "backups"), ...files(".bash_history", ".bashrc", ".profile")],
  "/home/deploy/apps": [{ name: "shop-admin", dir: true, project: true }, { name: "worker", dir: true, project: true }],
  "/home/deploy/backups": files("shop-2026-10-19.sql.gz", "shop-2026-10-20.sql.gz"),
  "/var": dirs("cache", "lib", "log", "www"),
  "/var/www": [
    { name: "html", dir: true },
    { name: "shop", dir: true, project: true },
    { name: "shop-staging", dir: true, project: true },
  ],
  "/srv": dirs("releases"),
};

let projectFiles: string[] = [];

/** Top-level names of the demo project (set by workspaceMock to avoid an import cycle). */
export function setRemoteProjectFiles(paths: string[]): void {
  projectFiles = paths;
}

function listProject(dir: string): { name: string; dir: boolean }[] {
  const rel = dir === REMOTE_PATH ? "" : dir.slice(REMOTE_PATH.length + 1);
  const prefix = rel ? `${rel}/` : "";
  const seen = new Map<string, boolean>();
  for (const p of projectFiles) {
    if (!p.startsWith(prefix)) continue;
    const [head, ...rest] = p.slice(prefix.length).split("/");
    seen.set(head, (seen.get(head) ?? false) || rest.length > 0);
  }
  return [...seen].map(([name, isDir]) => ({ name, dir: isDir }));
}

function normalize(path: string, home: string): string {
  const abs = path === "~" || path === "" ? home : path.startsWith("~/") ? `${home}/${path.slice(2)}` : path;
  const out: string[] = [];
  for (const part of abs.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") out.pop();
    else out.push(part);
  }
  return `/${out.join("/")}`;
}

registerMock("remote_browse", ({ connectionId, path }): BrowseResult => {
  const host = connectionHosts.get(String(connectionId));
  const dir = normalize(String(path ?? "~"), `/home/${host?.user ?? REMOTE_USER}`);
  const raw = dir === REMOTE_PATH || dir.startsWith(`${REMOTE_PATH}/`) ? listProject(dir) : TREE[dir];
  if (!raw) throw new Error(`io error at ${dir}: No such file or directory (os error 2)`);
  const entries: BrowseEntry[] = raw
    .map((e) => ({ ...e, path: `${dir === "/" ? "" : dir}/${e.name}`, project: "project" in e ? !!e.project : false }))
    .sort((a, b) => Number(!a.dir) - Number(!b.dir) || a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
  const parent = dir === "/" ? null : dir.slice(0, dir.lastIndexOf("/")) || "/";
  return { path: dir, parent, entries };
});

// ---- the remote project --------------------------------------------------------------------

let openProject: { connectionId: string; info: WorkspaceInfo } | null = null;
let latency = 38;

/** Workspace info of the remote demo project at `path`. */
export function remoteWorkspace(
  path = REMOTE_PATH,
  host: { host: string; port: number; user: string; configId?: string } = {
    host: REMOTE_HOST,
    port: 22,
    user: REMOTE_USER,
    configId: "shop-key",
  },
): WorkspaceInfo {
  const label = host.port === 22 ? `${host.user}@${host.host}` : `${host.user}@${host.host}:${host.port}`;
  return {
    name: path.split("/").pop() || path,
    root: `${host.user}@${host.host}:${path}`,
    remote: { ...host, label, path },
  };
}

function status(): RemoteStatus | null {
  if (!openProject?.info.remote) return null;
  return {
    connectionId: openProject.connectionId,
    label: openProject.info.remote.label,
    host: openProject.info.remote.host,
    state: "connected",
    latencyMs: latency,
  };
}

/** Make the remote demo project the open one (`?project=remote`, `remote_open_project`). */
export function adoptRemoteProject(info: WorkspaceInfo, connectionId = "conn-demo"): WorkspaceInfo {
  openProject = { connectionId, info };
  // The framework balloon would cover screenshots; announce only when asked.
  if (!new URLSearchParams(location.search).has("announce")) {
    setSetting("project.announcedFrameworks", [`${info.root}#laravel`], "user");
  }
  queueMicrotask(() => {
    const s = status();
    if (s) emitMock("remote://status", s);
  });
  return info;
}

/** The open remote project's user and folder (the fake terminal runs "there"). */
export function currentRemote(): { user: string; path: string } | null {
  const r = openProject?.info.remote;
  return r ? { user: r.user, path: r.path } : null;
}

/** Forget the remote project (a local project was opened). */
export function closeRemoteProject(): void {
  openProject = null;
}

registerMock("remote_open_project", async ({ connectionId, path }) => {
  await wait(150);
  const host = connectionHosts.get(String(connectionId));
  if (!host) throw new Error(`no connection ${connectionId}`);
  connectionHosts.delete(String(connectionId));
  return adoptRemoteProject(remoteWorkspace(String(path), host), String(connectionId));
});

registerMock("remote_status", () => status());

registerMock("remote_reconnect", async () => {
  const s = status();
  if (!s) throw new Error("the project is not on a remote host");
  emitMock("remote://status", { ...s, state: "reconnecting", attempt: 1, error: "Connection lost", nextRetrySecs: 1 });
  setTimeout(() => {
    latency = 41;
    const now = status();
    if (now) emitMock("remote://status", now);
  }, 1500);
});
