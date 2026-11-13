/**
 * Browser-mode Dev Containers and WSL. `devcontainer.json` of the demo project, the
 * `docker ps` list and `wsl.exe -l -v` (UTF-16 bytes) are parsed by the real Rust code
 * (WebAssembly); the containers, builds and distributions are fake and in memory. Opening a
 * container or a WSL folder shows the demo project as a remote project. Nothing touches
 * Docker, WSL or real files.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type {
  BuildLogLine,
  DevContainerConfigView,
  DevContainerInstance,
  DevContainerList,
  LinuxPath,
  ResolvedDevContainer,
  UpOutcome,
  WslConnected,
  WslDistro,
  WslInfo,
} from "@/ipc/devenv";
import type { BrowseEntry, BrowseResult } from "@/ipc/remote";
import type { TrustCheck } from "@/ipc/trust";
import type { WorkspaceInfo } from "@/ipc/workspace";
import { callWasm, forgeWasm } from "@/wasm";
import { adoptRemoteProject } from "./remoteMock";
import { mockFiles, mockProjectName } from "./workspaceMock";

const DEMO_FOLDER = `~/Projects/${mockProjectName}`;
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

// ---- devcontainer.json -----------------------------------------------------------------------

async function resolve(folder: string, rel: string): Promise<DevContainerConfigView> {
  const path = `${folder}/${rel}`;
  const text = mockFiles[rel];
  const out = await callWasm<{ config?: ResolvedDevContainer; source?: string; error?: string }>(
    (await forgeWasm()).devcontainer_resolve,
    { text, folder, configPath: path },
  );
  return { path, rel, ...out };
}

registerMock("devcontainer_configs", async ({ folder }): Promise<DevContainerConfigView[]> => {
  const base = String(folder || DEMO_FOLDER);
  // Every folder of the demo is the demo project.
  const rels = Object.keys(mockFiles).filter(
    (p) => p === ".devcontainer.json" || /^\.devcontainer\/([^/]+\/)?devcontainer\.json$/.test(p),
  );
  return Promise.all(rels.sort().map((rel) => resolve(base, rel)));
});

registerMock("devcontainer_gutter", async ({ path, content }) =>
  callWasm<number | null>((await forgeWasm()).devcontainer_gutter, { path, content }),
);

// ---- containers ------------------------------------------------------------------------------

interface FakeContainer {
  containerId: string;
  name: string;
  displayName: string;
  image: string;
  state: "running" | "exited";
  status: string;
  created: string;
  id: string;
  workspace: string;
  user: string;
  folder: string;
  config: string;
  compose: string;
}

const containers: FakeContainer[] = [
  {
    containerId: "3f9c2a1b7d4e",
    name: "forge-storefront-ui-5d1e09c3",
    displayName: "Storefront UI",
    image: "mcr.microsoft.com/devcontainers/typescript-node:22",
    state: "running",
    status: "Up 2 hours",
    created: "2026-10-26 08:12:40 +0100 CET",
    id: "5d1e09c3a7b2",
    workspace: "/workspaces/storefront-ui",
    user: "node",
    folder: "~/Projects/storefront-ui",
    config: "~/Projects/storefront-ui/.devcontainer/devcontainer.json",
    compose: "",
  },
  {
    containerId: "a81e44c0f6d2",
    name: "billing-api_devcontainer-app-1",
    displayName: "Billing API",
    image: "billing-api_devcontainer-app",
    state: "exited",
    status: "Exited (0) 3 days ago",
    created: "2026-10-22 17:03:11 +0100 CET",
    id: "c47aa1f2e9d0",
    workspace: "/var/www/html",
    user: "www-data",
    folder: "~/Projects/billing-api",
    config: "~/Projects/billing-api/.devcontainer/devcontainer.json",
    compose: "billing-api_devcontainer",
  },
];

/** What `docker ps` with Forge's format prints for the fake containers. */
function psOutput(): string {
  return containers
    .map((c) =>
      [
        c.containerId,
        c.name,
        c.image,
        c.state,
        c.status,
        c.created,
        c.id,
        c.displayName,
        c.workspace,
        c.user,
        c.folder,
        c.config,
        c.compose,
      ].join("\t"),
    )
    .join("\n");
}

registerMock("devcontainer_list", async (): Promise<DevContainerList> => {
  const containersParsed = JSON.parse((await forgeWasm()).devcontainer_parse_list(psOutput())) as DevContainerInstance[];
  return { available: true, containers: containersParsed };
});

function find(containerId: string): FakeContainer {
  const c = containers.find((x) => x.containerId.startsWith(containerId) || containerId.startsWith(x.containerId));
  if (!c) throw new Error(`There is no dev container ${containerId}`);
  return c;
}

function requireTrusted(folder: string): void {
  const check = mockHandler("trust_check")?.({ path: folder }) as TrustCheck | undefined;
  if (check && !check.trusted)
    throw new Error(
      "Building and starting a dev container is not available in Safe Mode: the project is not trusted. Trust the project (Trust Project… in the Safe Mode banner) to run its code.",
    );
}

let building: { cancelled: boolean } | null = null;

const log = (line: BuildLogLine) => emitMock("devcontainer://log", line);
const out = (text: string, stream: "stdout" | "stderr" = "stderr") => log({ kind: "output", stream, text: `${text}\n` });

registerMock("devcontainer_up", async ({ folder, configPath, rebuild }): Promise<UpOutcome> => {
  requireTrusted(String(folder));
  const view = await resolve(String(folder), String(configPath).slice(String(folder).length + 1));
  if (!view.config) throw new Error(view.error ?? "devcontainer.json is not valid");
  const cfg = view.config;
  const job = { cancelled: false };
  building = job;
  const step = async (ms: number, f: () => void) => {
    await wait(ms);
    if (job.cancelled) throw new Error("The build was cancelled");
    f();
  };
  const existing = containers.find((c) => c.id === cfg.id);
  for (const w of cfg.warnings) log({ kind: "warning", text: w });
  log({ kind: "command", text: `docker ps -a --filter label=forge.devcontainer.id=${cfg.id} --format '{{.ID}}\t{{.State}}'` });
  if (existing && !rebuild) {
    if (existing.state !== "running") {
      await step(200, () => {
        log({ kind: "step", text: `Starting container ${existing.name}…` });
        log({ kind: "command", text: `docker start ${existing.containerId}` });
        existing.state = "running";
        existing.status = "Up Less than a second";
      });
    }
    return { containerId: existing.containerId, created: false };
  }
  if (existing) {
    log({ kind: "step", text: "Removing the old container…" });
    log({ kind: "command", text: `docker rm -f ${existing.containerId}` });
    containers.splice(containers.indexOf(existing), 1);
  }
  const tag = `forge-devcontainer-${cfg.id}`;
  await step(150, () => {
    log({ kind: "step", text: "Building image…" });
    log({ kind: "command", text: `docker build -f ${cfg.configPath.replace(/devcontainer\.json$/, "Dockerfile")} -t ${tag} --build-arg PHP_VERSION=8.3 --label forge.devcontainer.id=${cfg.id} ${cfg.localFolder}` });
  });
  const build = [
    "#0 building with \"default\" instance using docker driver",
    "#1 [internal] load build definition from Dockerfile",
    "#1 transferring dockerfile: 214B done",
    "#2 [internal] load metadata for mcr.microsoft.com/devcontainers/php:8.3",
    "#2 DONE 0.8s",
    "#3 [1/2] FROM mcr.microsoft.com/devcontainers/php:8.3@sha256:4f7e…",
    "#3 CACHED",
    "#4 [2/2] RUN apt-get update && apt-get install -y libicu-dev && docker-php-ext-install intl pdo_mysql",
    "#4 2.913 Reading package lists...",
    "#4 7.402 Setting up libicu-dev:amd64 (72.1-3) ...",
    "#4 18.20 Installing shared extensions: /usr/local/lib/php/extensions/no-debug-non-zts-20230831/",
    "#4 DONE 19.4s",
    "#5 exporting to image",
    `#5 naming to docker.io/library/${tag} done`,
    "#5 DONE 0.6s",
  ];
  for (const line of build) await step(60, () => out(line));
  const name = `forge-${mockProjectName}-${cfg.id.slice(0, 8)}`;
  const containerId = "9b4d7e21c0aa";
  await step(150, () => {
    log({ kind: "step", text: `Creating container ${name}…` });
    log({
      kind: "command",
      text: `docker run -d --name ${name} --label forge.devcontainer.id=${cfg.id} --mount ${cfg.workspaceMount} -e APP_ENV=local -p 127.0.0.1:8000:8000 -p 127.0.0.1:5173:5173 --entrypoint /bin/sh ${tag} -c '…'`,
    });
    out(`${containerId}5f0e3d8c4b1a29e7d6f5c4b3a291807f6e5d4c3b2a1`, "stdout");
  });
  const user = cfg.remoteUser ?? "root";
  await step(200, () => {
    log({ kind: "step", text: "Running postCreateCommand…" });
    log({ kind: "command", text: `docker exec -u ${user} -w ${cfg.workspaceFolder} ${containerId} /bin/sh -c 'composer install && npm ci'` });
  });
  for (const line of [
    "Installing dependencies from lock file (including require-dev)",
    "Package operations: 112 installs, 0 updates, 0 removals",
    "Generating optimized autoload files",
    "added 214 packages, and audited 215 packages in 6s",
  ])
    await step(400, () => out(line, "stdout"));
  await step(150, () => {
    log({ kind: "step", text: "Running postStartCommand…" });
    log({ kind: "command", text: `docker exec -u ${user} -w ${cfg.workspaceFolder} ${containerId} /bin/sh -c 'php artisan migrate --force'` });
    out("INFO  Nothing to migrate.", "stdout");
    log({ kind: "step", text: "Dev container is ready." });
  });
  containers.unshift({
    containerId,
    name,
    displayName: cfg.name,
    image: tag,
    state: "running",
    status: "Up Less than a second",
    created: "2026-10-26 10:41:02 +0100 CET",
    id: cfg.id,
    workspace: cfg.workspaceFolder,
    user,
    folder: cfg.localFolder,
    config: cfg.configPath,
    compose: "",
  });
  building = null;
  return { containerId, created: true };
});

registerMock("devcontainer_cancel", () => {
  if (building) building.cancelled = true;
  building = null;
});

registerMock("devcontainer_action", async ({ containerId, action }) => {
  const c = find(String(containerId));
  await wait(120);
  if (action === "start") {
    requireTrusted(c.folder);
    c.state = "running";
    c.status = "Up Less than a second";
  } else if (action === "stop") {
    c.state = "exited";
    c.status = "Exited (0) Less than a second ago";
  } else if (action === "remove") {
    containers.splice(containers.indexOf(c), 1);
  } else throw new Error(`unknown action ${action}`);
});

export function devContainerWorkspace(c: Pick<FakeContainer, "containerId" | "displayName" | "workspace" | "user" | "folder" | "config">): WorkspaceInfo {
  return {
    name: c.workspace.split("/").pop() || c.workspace,
    root: `${c.displayName} [Dev Container]:${c.workspace}`,
    remote: {
      host: c.displayName,
      port: 0,
      user: c.user,
      label: `Dev Container: ${c.displayName}`,
      path: c.workspace,
      kind: { type: "devContainer", container: c.containerId, source: c.folder, config: c.config },
    },
  };
}

registerMock("devcontainer_open", async ({ containerId }) => {
  const c = find(String(containerId));
  emitMock("remote://progress", { message: `Checking the dev container ${c.displayName}…` });
  await wait(150);
  emitMock("remote://progress", { message: "Starting the Forge agent…" });
  await wait(150);
  c.state = "running";
  return adoptRemoteProject(devContainerWorkspace(c), "local-1");
});

// ---- WSL -------------------------------------------------------------------------------------

/** `wsl.exe -l -v` as it arrives on a pipe: UTF-16LE with a BOM and CRLF line ends. */
function wslListBytes(): Uint8Array {
  const text =
    "  NAME              STATE           VERSION\r\n* Ubuntu-24.04      Running         2\r\n  Debian            Stopped         2\r\n  docker-desktop    Running         2\r\n";
  const bytes = new Uint8Array(2 + text.length * 2);
  bytes[0] = 0xff;
  bytes[1] = 0xfe;
  for (let i = 0; i < text.length; i++) {
    const code = text.charCodeAt(i);
    bytes[2 + i * 2] = code & 0xff;
    bytes[3 + i * 2] = code >> 8;
  }
  return bytes;
}

registerMock("wsl_distributions", async (): Promise<WslInfo> => {
  const distros = JSON.parse((await forgeWasm()).wsl_parse_list(wslListBytes())) as WslDistro[];
  return { available: true, distros };
});

registerMock("wsl_map_path", async ({ path }) => JSON.parse((await forgeWasm()).wsl_map_path(String(path))) as LinuxPath | null);
registerMock("wsl_windows_path", async ({ distro, path }) => callWasm<string>((await forgeWasm()).wsl_windows_path, { distro, path }));

const wslConnections = new Map<string, string>();
let wslSeq = 0;

registerMock("wsl_connect", async ({ distro }): Promise<WslConnected> => {
  emitMock("remote://progress", { message: `Checking the WSL ${distro}…` });
  await wait(200);
  const id = `wsl-${++wslSeq}`;
  wslConnections.set(id, String(distro));
  return { connectionId: id, label: `WSL: ${distro}`, home: "/home/demo" };
});

const dirs = (...names: string[]) => names.map((n) => ({ name: n, dir: true, project: false }));
const WSL_TREE: Record<string, { name: string; dir: boolean; project: boolean }[]> = {
  "/": dirs("bin", "etc", "home", "mnt", "opt", "tmp", "usr", "var"),
  "/home": dirs("demo"),
  "/home/demo": [
    ...dirs(".cache", ".config", "go", "projects"),
    { name: ".bashrc", dir: false, project: false },
    { name: ".profile", dir: false, project: false },
  ],
  "/home/demo/projects": [
    { name: mockProjectName, dir: true, project: true },
    { name: "billing-api", dir: true, project: true },
    { name: "notes", dir: true, project: false },
  ],
  "/mnt": dirs("c", "wsl", "wslg"),
};

registerMock("wsl_browse", ({ connectionId, path }): BrowseResult => {
  if (!wslConnections.has(String(connectionId))) throw new Error(`no connection ${connectionId}`);
  const raw = String(path ?? "~");
  const dir = raw === "~" || raw === "" ? "/home/demo" : raw.replace(/\/+$/, "") || "/";
  const list = WSL_TREE[dir];
  if (!list) throw new Error(`io error at ${dir}: No such file or directory (os error 2)`);
  const entries: BrowseEntry[] = list.map((e) => ({ ...e, path: `${dir === "/" ? "" : dir}/${e.name}` }));
  const parent = dir === "/" ? null : dir.slice(0, dir.lastIndexOf("/")) || "/";
  return { path: dir, parent, entries };
});

export function wslWorkspace(distro: string, path: string): WorkspaceInfo {
  const windows = uncPath(distro, path);
  return {
    name: path.split("/").pop() || path,
    root: windows,
    remote: { host: distro, port: 0, user: "demo", label: `WSL: ${distro}`, path, kind: { type: "wsl", distro } },
  };
}

/** `\\wsl.localhost\<distro>\…`, as `forge_backend::RemoteHost::display_path` shows it. */
function uncPath(distro: string, path: string): string {
  return `\\\\wsl.localhost\\${distro}\\${path.split("/").filter(Boolean).join("\\")}`;
}

registerMock("wsl_open_project", async ({ connectionId, distro, path }) => {
  if (!wslConnections.delete(String(connectionId))) throw new Error(`no connection ${connectionId}`);
  await wait(150);
  return adoptRemoteProject(wslWorkspace(String(distro), String(path)), String(connectionId));
});

registerMock("wsl_disconnect", ({ connectionId }) => {
  wslConnections.delete(String(connectionId));
});
