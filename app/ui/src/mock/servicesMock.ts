/**
 * Browser-mode Services backend: a fake Docker daemon (acme-shop compose with app, mysql and
 * redis, images, volumes, networks, two Docker contexts) and a kind cluster with pods; the
 * remote demo project shows the contexts of its host (a k3s next to the shop). The
 * models, the tree, details, gutter markers, command lines, port specs, log prefixes, pull
 * progress and console lines all come from the Rust crates through WebAssembly; this file
 * only keeps the fake daemon's state (and fake port forwards) and replays output. It never
 * touches real files, processes or ports.
 */
import { emitMock, registerMock } from "@/ipc";
import type { ConsoleLine } from "@/ipc/run";
import type { DockerContext, GutterMarker, KubeSelection, PortForward, ServiceAction, ServiceDetails, ServiceNode, ServicesEvent, TerminalCommand } from "@/ipc/services";
import { callWasmSync, forgeWasm, type ForgeWasm } from "@/wasm";
import { demoProjectName } from "./demoProject";
import { isSymfonyDemo, symfonyDemoName } from "./demoSymfonyProject";
import { mockSettingValue } from "./settingsMock";
import { mockFiles } from "./workspaceMock";
import { currentRemote, onRemoteProjectChanged } from "./remoteMock";
import {
  ago,
  composeContainer,
  demoContainers,
  demoImages,
  demoKubeconfig,
  hostKubeconfigView,
  demoNamespaces,
  demoNetworks,
  demoVersion,
  demoVolumes,
  demoWorkloads,
  initialLogs,
  nextLogLine,
  podLogs,
  type RawContainer,
} from "./servicesDemoData";

const ENDPOINT = "unix:///var/run/docker.sock";
const EVENT = "services://event";

/** `~/.docker/contexts` of the demo machine (meta.json texts). */
const CONTEXT_METAS = [
  { Name: "buildbox", Metadata: { Description: "CI build host" }, Endpoints: { docker: { Host: "ssh://ci@buildbox.acme.dev", SkipTLSVerify: false } } },
  { Name: "staging", Metadata: { Description: "Staging Swarm manager" }, Endpoints: { docker: { Host: "tcp://10.20.0.15:2376", SkipTLSVerify: false } } },
].map((m) => JSON.stringify(m));

const contexts = () => callWasmSync<DockerContext[]>(w().containers_docker_contexts, { metas: CONTEXT_METAS, current: null });

/** The endpoint label the settings select (the fake daemon answers on all of them). */
function endpoint(): string {
  const setting = (k: string) => String(mockSettingValue(k) ?? "");
  // A remote project uses the Docker daemon of its host through the project's connection.
  const remote = currentRemote();
  if (remote && mockSettingValue("docker.remoteProjectHost") !== false) {
    const socket = setting("docker.ssh.socketPath").trim() || "/var/run/docker.sock";
    return `ssh://${remote.label}${socket}`;
  }
  switch (setting("docker.connection")) {
    case "ssh": {
      const url = setting("docker.ssh.url").trim() || "ssh://deploy@build.acme.dev";
      const socket = setting("docker.ssh.socketPath").trim();
      return socket && socket !== "/var/run/docker.sock" ? `${url}${socket}` : url;
    }
    case "context":
      return contexts().find((c) => c.name === setting("docker.context"))?.host ?? ENDPOINT;
    case "tcp":
      return setting("docker.tcp.url") || "tcp://localhost:2375";
    default:
      return ENDPOINT;
  }
}

const docker = {
  containers: demoContainers(),
  images: demoImages() as ReturnType<typeof demoImages>,
  volumes: demoVolumes(),
  networks: demoNetworks(),
};
let busy: string[] = [];
let forwards: PortForward[] = [];
let nextForward = 0;
type Loaded<T> = { state: "loading" } | { state: "ready"; value: T } | { state: "failed"; error: string };
const namespaces: Record<string, Loaded<string[]>> = {};
const workloads: Record<string, Loaded<unknown>> = {};
const registryPasswords = new Set<string>();

let wasm: ForgeWasm | null = null;
const w = () => {
  if (!wasm) throw new Error("WebAssembly is not loaded yet");
  return wasm;
};
void forgeWasm().then((m) => (wasm = m));

const emit = (e: ServicesEvent) => emitMock(EVENT, e);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const projectName = () => (isSymfonyDemo() ? symfonyDemoName : demoProjectName);

// ---- input for the Rust tree ------------------------------------------------------------

const COMPOSE_NAMES = ["compose.yaml", "compose.yml", "docker-compose.yaml", "docker-compose.yml"];

function composeFiles() {
  const env = Object.fromEntries(
    (mockFiles[".env"] ?? "")
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l && !l.startsWith("#") && l.includes("="))
      .map((l) => [l.slice(0, l.indexOf("=")), l.slice(l.indexOf("=") + 1).replace(/^"|"$/g, "")]),
  );
  return Object.keys(mockFiles)
    .filter((p) => COMPOSE_NAMES.includes(p.split("/").pop()!) && p.split("/").length <= 2)
    .sort()
    .map((path) => {
      const dir = path.includes("/") ? path.split("/")[0] : projectName();
      return callWasmSync<{ project: string; services: { name: string; profiles: string[] }[] }>(w().containers_compose_parse, {
        path,
        content: mockFiles[path],
        env,
        project: dir,
      });
    });
}

/** The remote project host whose kubeconfig and kubectl are used, with that kubectl. */
function kubeHost(): { label: string; kubectl: string } | null {
  const remote = currentRemote();
  if (!remote || mockSettingValue("kubernetes.remoteProjectHost") === false) return null;
  const entry = ((mockSettingValue("kubernetes.hostKubectlPaths") as string[] | undefined) ?? [])
    .map((l) => l.split("\t"))
    .find(([h]) => h?.trim() === remote.label);
  return { label: remote.label, kubectl: entry?.[1]?.trim() || "kubectl" };
}

/** Context and namespace picked in the selector (this session). */
let kubeSelect: { context: string; namespace?: string; host: string | null } | null = null;
/** Clusters that answer: kind on this machine, the host's k3s. */
const REACHABLE = ["kind-acme", "k3s-shop"];
/** The namespace with the demo workloads per context. */
const SHOP_NS: Record<string, string> = { "kind-acme": "shop", "k3s-shop": "shop" };

function kubeInput() {
  const host = kubeHost();
  // A remote project: `kubectl config view -o json` on the host.
  const config = host
    ? callWasmSync(w().containers_kubeconfig_parse, { content: hostKubeconfigView, baseDir: "" })
    : callWasmSync(w().containers_kubeconfig_parse, { content: demoKubeconfig, baseDir: "/home/dev/.kube" });
  const selected = kubeSelect && kubeSelect.host === (host?.label ?? null) ? kubeSelect : null;
  return {
    path: host ? "$KUBECONFIG or ~/.kube/config" : "~/.kube/config",
    config,
    defaultContext: selected?.context ?? ((mockSettingValue("kubernetes.defaultContext") as string) || undefined),
    defaultNamespace: selected ? selected.namespace : (mockSettingValue("kubernetes.defaultNamespace") as string) || undefined,
    namespaces,
    workloads,
    host: host?.label,
    kubectl: host?.kubectl,
  };
}

function input() {
  const snapshot = callWasmSync(w().containers_docker_snapshot, { ...docker, version: demoVersion });
  return {
    docker: { state: "connected", endpoint: endpoint(), snapshot, contexts: contexts() },
    composeFiles: composeFiles(),
    kube: kubeInput(),
    now: Math.floor(Date.now() / 1000),
    busy,
    portForwards: forwards,
  };
}

const tree = () => callWasmSync<ServiceNode[]>(w().services_tree, input());
const emitTree = () => emit({ type: "tree", nodes: tree() });

/** Node id → target, using the same Rust parser as the details pane. */
function target(nodeId: string) {
  return details(nodeId)?.target ?? null;
}

const details = (nodeId: string) => callWasmSync<ServiceDetails | null>(w().services_details, { input: input(), nodeId });

// ---- consoles -----------------------------------------------------------------------------

interface MockConsole {
  handle: number;
  lines: ConsoleLine[];
}
const consoles = new Map<string, MockConsole>();
const streams = new Map<string, ReturnType<typeof setInterval>>();

function consoleFor(key: string, reset = false): MockConsole {
  let c = consoles.get(key);
  if (!c || reset) {
    if (c) w().run_output_drop(JSON.stringify({ handle: c.handle }));
    c = { handle: callWasmSync<number>(w().run_output_new, { root: `~/Projects/${projectName()}` }), lines: [] };
    consoles.set(key, c);
  }
  return c;
}

type RunInput =
  | { type: "started"; commandLine: string }
  | { type: "output"; stream: "stdout" | "stderr"; text: string }
  | { type: "exited"; code: number | null; signal: number | null };

function feed(key: string, input: RunInput, reset = false): void {
  const c = consoleFor(key, reset);
  const events = callWasmSync<{ type: string; replaceLast?: boolean; lines?: ConsoleLine[] }[]>(w().run_output_handle, { handle: c.handle, input });
  for (const e of events) {
    if (e.type !== "console") continue;
    const delta = { replaceLast: !!e.replaceLast, lines: e.lines ?? [] };
    if (delta.replaceLast && delta.lines.length) c.lines.pop();
    c.lines.push(...delta.lines);
    emit({ type: "console", key, delta, reset });
    reset = false;
  }
  if (reset) emit({ type: "console", key, delta: { replaceLast: false, lines: [] }, reset: true });
}

const out = (key: string, text: string, stream: "stdout" | "stderr" = "stdout") => feed(key, { type: "output", stream, text });

// ---- fake daemon ----------------------------------------------------------------------------

const containerById = (id: string) => docker.containers.find((c) => c.Id === id);
const serviceOf = (c: RawContainer) => c.Labels["com.docker.compose.service"] ?? c.Names[0].replace(/^\//, "");
const composeContainers = (project: string, service?: string) =>
  docker.containers.filter((c) => c.Labels["com.docker.compose.project"] === project && (!service || c.Labels["com.docker.compose.service"] === service));

function setState(c: RawContainer, running: boolean): void {
  c.State = running ? "running" : "exited";
  c.Status = running ? "Up Less than a second" : "Exited (0) Less than a second ago";
}

async function withBusy<T>(ids: string[], fn: () => Promise<T>): Promise<T> {
  busy = [...busy, ...ids];
  emitTree();
  try {
    return await fn();
  } finally {
    busy = busy.filter((b) => !ids.includes(b));
    emitTree();
  }
}

const IMAGES: Record<string, string> = { app: `${demoProjectName}-app`, mysql: "mysql:8.4", redis: "redis:7-alpine" };
const IPS: Record<string, string> = { app: "172.18.0.4", mysql: "172.18.0.2", redis: "172.18.0.3" };

/** `docker compose …` with plausible output (Compose v2 plain progress). */
async function compose(project: string, action: string, services: string[]): Promise<void> {
  const projectNode = `compose/${encodeURIComponent(project)}`;
  const key = `run:${projectNode}`;
  const file = composeFiles().find((f) => f.project === project);
  const all = services.length ? services : (file?.services.map((s) => s.name) ?? [...new Set(composeContainers(project).map(serviceOf))]);
  const args = callWasmSync<string[]>(w().containers_compose_args, {
    files: ["docker-compose.yml"],
    project,
    profiles: [],
    action,
    services,
    build: false,
  });
  const busyIds = services.length ? services.map((s) => `${projectNode}/${encodeURIComponent(s)}`) : [projectNode];
  await withBusy(busyIds, async () => {
    feed(key, { type: "started", commandLine: `docker ${args.join(" ")}` });
    const remote = currentRemote();
    if (remote) out(key, `Running on ${remote.label} in ${remote.path}\n`);
    await sleep(250);
    if (action === "up" || action === "start" || action === "restart") {
      const verb = action === "restart" ? "Restarted" : "Started";
      out(key, ` Network ${project}_default  ${action === "up" ? "Created" : "Running"}\n`);
      for (const s of all) {
        await sleep(220);
        let c = composeContainers(project, s)[0];
        if (!c) {
          c = composeContainer(s, IMAGES[s] ?? `${s}:latest`, { ip: IPS[s] ?? "172.18.0.9", Created: ago(0) });
          docker.containers.push(c);
          out(key, ` Container ${c.Names[0].slice(1)}  Created\n`);
        }
        setState(c, true);
        out(key, ` Container ${c.Names[0].slice(1)}  ${verb}\n`);
        emitTree();
      }
    } else if (action === "stop" || action === "down") {
      for (const s of [...all].reverse()) {
        await sleep(220);
        const c = composeContainers(project, s)[0];
        if (!c) continue;
        setState(c, false);
        out(key, ` Container ${c.Names[0].slice(1)}  Stopped\n`);
        if (action === "down") {
          docker.containers = docker.containers.filter((x) => x !== c);
          out(key, ` Container ${c.Names[0].slice(1)}  Removed\n`);
        }
        emitTree();
      }
      if (action === "down" && !services.length) out(key, ` Network ${project}_default  Removed\n`);
    } else if (action === "pull") {
      for (const s of all) out(key, ` ${s} Pulled\n`);
    }
    feed(key, { type: "exited", code: 0, signal: null });
    emit({ type: "ended", key });
  });
}

async function pull(image: string): Promise<void> {
  const key = "run:docker";
  feed(key, { type: "started", commandLine: `Pulling image ${image}` });
  const handle = callWasmSync<number>(w().containers_pull_new, { image });
  const tag = image.includes(":") ? image.split(":").pop()! : "latest";
  const layers = ["a1b2c3d4e5f6", "0f9e8d7c6b5a", "5c4d3e2f1a0b"];
  const messages: object[] = [{ status: `Pulling from library/${image.split(":")[0]}`, id: tag }];
  for (const l of layers) messages.push({ status: "Pulling fs layer", id: l });
  for (let step = 1; step <= 4; step++)
    for (const l of layers) messages.push({ status: "Downloading", id: l, progressDetail: { current: step * 2_500_000, total: 10_000_000 } });
  for (const l of layers) messages.push({ status: "Pull complete", id: l });
  messages.push({ status: `Digest: sha256:4c5d9a8b7e6f1d2c3b4a59687f6e5d4c3b2a1908f7e6d5c4b3a2918f7e6d5c4b` });
  messages.push({ status: `Status: Downloaded newer image for ${image.includes(":") ? image : `${image}:latest`}` });
  for (const m of messages) {
    await sleep(40);
    const r = callWasmSync<{ lines: string[] }>(w().containers_pull_push, { handle, text: `${JSON.stringify(m)}\n` });
    for (const l of r.lines) out(key, `${l}\n`);
  }
  const done = callWasmSync<{ lines: string[] }>(w().containers_pull_push, { handle, text: null });
  for (const l of done.lines) out(key, `${l}\n`);
  out(key, `Image ${image} pulled\n`);
  emit({ type: "ended", key });
  const full = image.includes(":") ? image : `${image}:latest`;
  if (!docker.images.some((i) => i.RepoTags.includes(full)))
    docker.images.push({ Id: `sha256:${Math.random().toString(16).slice(2).padEnd(64, "0")}`, RepoTags: [full], RepoDigests: [], Created: ago(5 * 24 * 3600), Size: 52_400_000, Containers: 0 });
  emitTree();
}

async function action(nodeId: string, act: ServiceAction): Promise<void> {
  const t = target(nodeId);
  if (!t) throw new Error(`Unknown node ${nodeId}`);
  const composeAction = (a: ServiceAction) =>
    ({ up: "up", start: "up", stop: "stop", restart: "restart", down: "down", remove: "down", pull: "pull" })[a as string];
  switch (t.kind) {
    case "container": {
      const c = containerById(t.id);
      if (!c) throw new Error("No such container");
      if (act === "pull") return pull(c.Image);
      await withBusy([nodeId], async () => {
        await sleep(400);
        if (act === "start" || act === "restart") setState(c, true);
        else if (act === "stop") setState(c, false);
        else if (act === "remove") docker.containers = docker.containers.filter((x) => x !== c);
      });
      return;
    }
    case "image": {
      const img = docker.images.find((i) => i.Id === t.id);
      if (act === "pull" && img) return pull(img.RepoTags[0]);
      if (act === "remove") {
        if (docker.containers.some((c) => img?.RepoTags.includes(c.Image)))
          throw new Error(`conflict: unable to remove repository reference "${img?.RepoTags[0]}" (must force) - container is using its referenced image`);
        docker.images = docker.images.filter((i) => i.Id !== t.id);
      }
      break;
    }
    case "volume":
      if (docker.containers.some((c) => c.Mounts.some((m) => m.Name === t.name))) throw new Error(`remove ${t.name}: volume is in use`);
      docker.volumes.Volumes = docker.volumes.Volumes.filter((v) => v.Name !== t.name);
      break;
    case "network":
      docker.networks = docker.networks.filter((n) => n.Id !== t.id);
      break;
    case "composeProject":
      return compose(t.project, composeAction(act) ?? "up", []);
    case "composeService":
      return compose(t.project, composeAction(act) ?? "up", [t.service]);
    case "pod":
      if (act === "remove") {
        await sleep(300);
        const key = `${t.context}\u001f${t.namespace}`;
        const w0 = workloads[key];
        if (w0?.state === "ready") {
          const value = w0.value as { pods: { name: string; status: string }[] };
          value.pods = value.pods.map((p) => (p.name === t.name ? { ...p, status: "Terminating" } : p));
        }
      }
      break;
    case "kubeContext":
    case "kubeNamespace":
    case "kubeRoot":
    case "kubeGroup":
      if (t.kind === "kubeContext") await loadNamespaces(t.context);
      else if (t.kind !== "kubeRoot") await loadWorkloads(t.context, t.namespace);
      break;
  }
  emitTree();
}

// ---- Kubernetes ---------------------------------------------------------------------------

async function loadNamespaces(context: string): Promise<void> {
  namespaces[context] = { state: "loading" };
  emitTree();
  await sleep(250);
  namespaces[context] = REACHABLE.includes(context)
    ? { state: "ready", value: demoNamespaces.items.map((n) => n.metadata.name) }
    : { state: "failed", error: unreachable() };
}

async function loadWorkloads(context: string, ns: string): Promise<void> {
  const key = `${context}\u001f${ns}`;
  workloads[key] = { state: "loading" };
  emitTree();
  await sleep(200);
  if (!REACHABLE.includes(context)) {
    workloads[key] = { state: "failed", error: unreachable() };
    return;
  }
  const raw = ns === SHOP_NS[context] ? demoWorkloads() : { pods: { items: [] }, deployments: { items: [] }, services: { items: [] } };
  workloads[key] = { state: "ready", value: callWasmSync(w().containers_workloads_parse, raw) };
}

/** How the other contexts fail: the API server from here, kubectl on the host. */
const unreachable = () =>
  kubeHost()
    ? "error: You must be logged in to the server (the server has asked for the client to provide credentials)"
    : "Cannot run the credential plugin `kubelogin`: No such file or directory (os error 2)";

const kubeSelection = () => callWasmSync<KubeSelection | null>(w().services_kube_selection, kubeInput());

async function expand(nodeId: string): Promise<ServiceNode[]> {
  const t = target(nodeId);
  if (t?.kind === "kubeContext") {
    await loadNamespaces(t.context);
    const sel = kubeSelection();
    const ns = sel?.context === t.context && sel.namespace ? sel.namespace : SHOP_NS[t.context] ?? "default";
    await loadWorkloads(t.context, ns);
  } else if (t?.kind === "kubeNamespace") await loadWorkloads(t.context, t.namespace);
  const nodes = tree();
  emit({ type: "tree", nodes });
  return nodes;
}

// ---- logs -----------------------------------------------------------------------------------

/** `kubectl logs deploy/<name> --all-containers --prefix`: the deployment's pods (by its
 * selector, from the Rust model) and every container, prefixed by Rust. */
function deploymentLogs(nodeId: string, key: string): void {
  const d = details(nodeId);
  const pods = d?.sections.find((s) => s.title === "Pods")?.table?.rows.map((r) => r[0]) ?? [];
  if (!pods.length) out(key, `No pods of deployment ${d?.title ?? ""} found\n`);
  const raw = demoWorkloads().pods.items;
  for (const pod of pods) {
    const containers = raw.find((p) => p.metadata.name === pod)?.spec.containers.map((c) => c.name) ?? [];
    for (const container of containers) {
      const text = podLogs(pod, container).join("");
      out(key, callWasmSync<string>(w().containers_prefix_log, { pod, container, text }));
    }
  }
}

function startLogs(nodeId: string, container?: string): string {
  const t = target(nodeId);
  const key = `log:${nodeId}`;
  clearInterval(streams.get(key));
  feed(key, { type: "output", stream: "stdout", text: "" }, true);
  let service: string | null = null;
  let lines: string[] = [];
  let running = false;
  if (t?.kind === "container" || t?.kind === "composeService") {
    const c = t.kind === "container" ? containerById(t.id) : composeContainers(t.project, t.service)[0];
    if (!c) throw new Error("The service has no container yet");
    service = serviceOf(c);
    lines = initialLogs(service);
    running = c.State === "running";
  } else if (t?.kind === "pod") {
    const choices = details(nodeId)?.containers ?? [];
    const picked = choices.find((c) => c.name === container) ?? choices.find((c) => c.default);
    lines = podLogs(t.name, picked?.name);
    running = !t.name.startsWith("migrate");
  } else if (t?.kind === "deployment") {
    deploymentLogs(nodeId, key);
    return key;
  } else throw new Error("This node has no logs");
  for (const l of lines) out(key, l, l.includes("ERROR") || l.includes("\x1b[31m") ? "stderr" : "stdout");
  if (running && service) {
    let n = 0;
    streams.set(
      key,
      setInterval(() => {
        const line = nextLogLine(service!, n++);
        if (line) out(key, line);
      }, 2000),
    );
  } else if (!running) emit({ type: "ended", key });
  return key;
}

// ---- inspect ----------------------------------------------------------------------------------

function inspect(nodeId: string): string {
  const t = target(nodeId);
  const iso = (s: number) => new Date(s * 1000).toISOString();
  if (t?.kind === "container" || t?.kind === "composeService") {
    const c = t.kind === "container" ? containerById(t.id) : composeContainers(t.project, t.service)[0];
    if (!c) throw new Error("No such container");
    const [path, ...args] = c.Command.split(" ");
    return JSON.stringify(
      {
        Id: c.Id,
        Created: iso(c.Created),
        Path: path,
        Args: args,
        State: { Status: c.State, Running: c.State === "running", Paused: false, Restarting: false, OOMKilled: false, Dead: false, Pid: c.State === "running" ? 2817 : 0, ExitCode: 0, StartedAt: iso(c.Created + 4), FinishedAt: "0001-01-01T00:00:00Z" },
        Image: c.ImageID,
        Name: c.Names[0],
        RestartCount: 0,
        Driver: "overlay2",
        Platform: "linux",
        HostConfig: {
          NetworkMode: Object.keys(c.NetworkSettings.Networks)[0],
          PortBindings: Object.fromEntries(c.Ports.filter((p) => p.PublicPort && p.IP !== "::").map((p) => [`${p.PrivatePort}/${p.Type}`, [{ HostIp: "", HostPort: String(p.PublicPort) }]])),
          RestartPolicy: { Name: "no", MaximumRetryCount: 0 },
        },
        Mounts: c.Mounts.map((m) => ({ Type: m.Type, Name: m.Name, Source: m.Source, Destination: m.Destination, Mode: "", RW: m.RW, Propagation: m.Type === "bind" ? "rprivate" : "" })),
        Config: { Hostname: c.Id.slice(0, 12), Tty: false, Env: c.Env ?? [], Cmd: args, Image: c.Image, WorkingDir: c.Image.endsWith("-app") ? "/var/www/html" : "", Labels: c.Labels },
        NetworkSettings: { Networks: Object.fromEntries(Object.entries(c.NetworkSettings.Networks).map(([n, v]) => [n, { IPAddress: v.IPAddress, Gateway: v.IPAddress.replace(/\.\d+$/, ".1"), IPPrefixLen: 16, MacAddress: "02:42:ac:12:00:04" }])) },
      },
      null,
      2,
    );
  }
  if (t?.kind === "image") return JSON.stringify(docker.images.find((i) => i.Id === t.id) ?? {}, null, 2);
  if (t?.kind === "volume") return JSON.stringify(docker.volumes.Volumes.find((v) => v.Name === t.name) ?? {}, null, 2);
  if (t?.kind === "network") return JSON.stringify(docker.networks.find((n) => n.Id === t.id) ?? {}, null, 2);
  if (t?.kind === "pod") return JSON.stringify(demoWorkloads().pods.items.find((p) => p.metadata.name === t.name) ?? {}, null, 2);
  if (t?.kind === "deployment") return JSON.stringify(demoWorkloads().deployments.items.find((p) => p.metadata.name === t.name) ?? {}, null, 2);
  if (t?.kind === "kubeService") return JSON.stringify(demoWorkloads().services.items.find((p) => p.metadata.name === t.name) ?? {}, null, 2);
  throw new Error("Nothing to inspect");
}

function terminalCommand(nodeId: string, attach: boolean, container?: string): TerminalCommand {
  const t = target(nodeId);
  if (t?.kind === "pod") {
    const choices = details(nodeId)?.containers ?? [];
    const picked = choices.find((c) => c.name === container && !c.init) ?? choices.find((c) => c.default);
    const host = kubeHost();
    const cmd = callWasmSync<TerminalCommand>(w().containers_terminal_command, { kind: "kubectl", context: t.context, namespace: t.namespace, pod: t.name, container: picked?.name, attach });
    // In a remote project the host's kubectl runs in a terminal on the host.
    return host ? { ...cmd, program: host.kubectl, env: [], onHost: true } : cmd;
  }
  const c = t?.kind === "container" ? containerById(t.id) : t?.kind === "composeService" ? composeContainers(t.project, t.service)[0] : undefined;
  if (!c) throw new Error("No container to open a terminal in");
  return callWasmSync(w().containers_terminal_command, { kind: attach ? "attach" : "exec", container: c.Id, name: c.Names[0].slice(1) });
}

/** The fake container shell of the terminal mock (`docker exec` / `docker attach`). */
export function mockContainerFor(id: string): { name: string; service: string; running: boolean } | null {
  const c = docker.containers.find((x) => x.Id === id || x.Names[0] === `/${id}`);
  return c ? { name: c.Names[0].slice(1), service: serviceOf(c), running: c.State === "running" } : null;
}

// ---- registration -----------------------------------------------------------------------------

registerMock("services_tree", () => tree());
registerMock("services_refresh", () => tree());
registerMock("services_compose_changed", () => emitTree());
registerMock("services_expand", ({ nodeId }) => expand(String(nodeId)));
registerMock("services_details", ({ nodeId }) => {
  const d = callWasmSync<ServiceDetails | null>(w().services_details, { input: input(), nodeId });
  return d;
});
registerMock("services_action", ({ nodeId, action: act }) => action(String(nodeId), act as ServiceAction));
registerMock("services_inspect", ({ nodeId }) => inspect(String(nodeId)));
registerMock("services_console", ({ key }) => consoles.get(String(key))?.lines ?? []);
registerMock("services_logs_start", ({ nodeId, container }) => startLogs(String(nodeId), container ? String(container) : undefined));
registerMock("services_logs_stop", ({ nodeId }) => {
  const key = `log:${String(nodeId)}`;
  clearInterval(streams.get(key));
  streams.delete(key);
});
registerMock("services_terminal", ({ nodeId, attach, container }) => terminalCommand(String(nodeId), !!attach, container ? String(container) : undefined));
// Port forwards are only bookkeeping here: nothing listens on the ports.
registerMock("services_port_forward_start", async ({ nodeId, spec }) => {
  const t = target(String(nodeId));
  if (t?.kind !== "pod") throw new Error("Ports can only be forwarded to pods");
  const pairs = callWasmSync<[number, number][]>(w().containers_port_forward_specs, { spec: String(spec) });
  await sleep(150);
  const started: PortForward[] = [];
  for (const [local, remote] of pairs) {
    const localPort = local || 49152 + Math.floor(Math.random() * 16000);
    if (forwards.some((f) => f.localPort === localPort)) throw new Error(`Local port ${localPort} is already in use`);
    const f: PortForward = { id: ++nextForward, nodeId: String(nodeId), context: t.context, namespace: t.namespace, pod: t.name, address: "127.0.0.1", localPort, remotePort: remote, connections: 0, via: kubeHost()?.label };
    started.push(f);
  }
  forwards = [...forwards, ...started];
  emitTree();
  return started;
});
registerMock("services_port_forward_stop", ({ id }) => {
  forwards = forwards.filter((f) => f.id !== Number(id));
  emitTree();
});
registerMock("services_port_forwards", () => forwards);
registerMock("docker_contexts", () => contexts());
registerMock("services_pull", ({ image }) => pull(String(image).trim()));
registerMock("services_compose_up", ({ path, service }) => {
  const f = composeFiles().find((x) => (x as unknown as { path: string }).path === path) ?? composeFiles()[0];
  if (!f) throw new Error(`${String(path)} is not a compose file`);
  return compose(f.project, "up", service ? [String(service)] : []);
});
registerMock("services_dockerfile_run", async ({ path }) => {
  const plan = callWasmSync<{ image: string; container: string; buildArgs: string[]; runArgs: string[] }>(w().containers_dockerfile_plan, {
    path,
    project: projectName(),
    content: mockFiles[String(path)] ?? "",
  });
  const key = "run:docker";
  await withBusy(["docker"], async () => {
    feed(key, { type: "started", commandLine: `docker ${plan.buildArgs.join(" ")}` });
    const steps = (mockFiles[String(path)] ?? "").split("\n").filter((l) => /^[A-Z]+\s/.test(l));
    out(key, "#0 building with \"default\" instance using docker driver\n");
    for (const [i, s] of steps.entries()) {
      await sleep(120);
      out(key, `#${i + 1} [${i + 1}/${steps.length}] ${s}\n#${i + 1} DONE ${(0.1 + i * 0.37).toFixed(1)}s\n`);
    }
    out(key, `#${steps.length + 1} naming to docker.io/library/${plan.image} done\n`);
    feed(key, { type: "exited", code: 0, signal: null });
    feed(key, { type: "started", commandLine: `docker ${plan.runArgs.join(" ")}` });
    const c = composeContainer(plan.container, plan.image, { ip: "172.17.0.2", Created: ago(0), Status: "Up Less than a second" });
    c.Names = [`/${plan.container}`];
    c.Labels = {};
    c.NetworkSettings = { Networks: { bridge: { IPAddress: "172.17.0.2" } } };
    docker.containers = docker.containers.filter((x) => x.Names[0] !== c.Names[0]);
    docker.containers.push(c);
    out(key, `${c.Id}\n`);
    feed(key, { type: "exited", code: 0, signal: null });
    emit({ type: "ended", key });
  });
});
// Opening (or leaving) the remote project switches Docker and Kubernetes to its host.
onRemoteProjectChanged(() => {
  if (!wasm) return;
  emitTree();
  emit({ type: "status", state: "connected", endpoint: endpoint(), version: demoVersion.Version });
});
registerMock("services_kube_selection", () => kubeSelection());
registerMock("services_kube_select", async ({ context, namespace }) => {
  const sel = kubeSelection();
  if (!sel?.contexts.includes(String(context))) throw new Error(`Context \`${String(context)}\` is not in the kubeconfig`);
  kubeSelect = { context: String(context), namespace: namespace ? String(namespace) : undefined, host: kubeHost()?.label ?? null };
  await expand(`kube/${encodeURIComponent(String(context))}`);
  return kubeSelection();
});
/** `kubectl apply -f` / `delete -f` with plausible output; the cluster does not change. */
registerMock("services_kube_apply", async ({ path, action }) => {
  const content = mockFiles[String(path)];
  if (content === undefined) throw new Error(`${String(path)} was not found`);
  const objects = [...content.matchAll(/^kind:\s*(\S+)[\s\S]*?^\s{2}name:\s*(\S+)/gm)].map((m) => `${m[1].toLowerCase()}${m[1] === "Deployment" ? ".apps" : ""}/${m[2]}`);
  if (!objects.length) throw new Error(`${String(path)} has no Kubernetes objects (apiVersion and kind)`);
  const sel = kubeSelection();
  if (!sel?.context) throw new Error("No Kubernetes context to apply the manifest to: check the kubeconfig");
  const host = kubeHost();
  const key = "run:kube";
  const ns = /^\s{2}namespace:/m.test(content) ? "" : ` -n ${sel.namespace ?? "default"}`;
  await withBusy(["kube"], async () => {
    feed(key, { type: "started", commandLine: `${host?.kubectl ?? "kubectl"} --context ${sel.context}${ns} ${String(action)} -f ${String(path)}` });
    const remote = currentRemote();
    if (host && remote) out(key, `Running on ${remote.label} in ${remote.path}\n`);
    for (const o of objects) {
      await sleep(180);
      out(key, `${o} ${action === "delete" ? "deleted" : o.startsWith("service/") ? "unchanged" : "configured"}\n`);
    }
    feed(key, { type: "exited", code: 0, signal: null });
    emit({ type: "ended", key });
  });
});
registerMock("services_gutter", ({ path, content }) => callWasmSync<GutterMarker[]>(w().services_gutter, { path, content }));
registerMock("services_status", () => ({ type: "status", state: "connected", endpoint: endpoint(), version: demoVersion.Version }));
registerMock("docker_test_connection", async ({ settings }) => {
  await sleep(300);
  const s = settings as { connection?: string; tcpUrl?: string; sshUrl?: string; sshSocket?: string; context?: string } | undefined;
  if (s?.connection === "tcp") throw new Error(`Cannot connect to ${s.tcpUrl?.replace(/^tcp:\/\//, "") ?? "localhost:2375"}: Connection refused (os error 111)`);
  let at = ENDPOINT;
  if (s?.connection === "ssh") {
    if (!s.sshUrl?.trim()) throw new Error("Enter the SSH host of the Docker daemon (ssh://user@host)");
    at = s.sshUrl.trim();
  } else if (s?.connection === "context") {
    const ctx = contexts().find((c) => c.name === s.context);
    if (!ctx) throw new Error(`Docker context \`${s.context ?? ""}\` not found`);
    at = ctx.host;
  }
  return `Connection successful: ${demoVersion.Platform.Name} ${demoVersion.Version} (API ${demoVersion.ApiVersion}) at ${at}`;
});
registerMock("docker_registry_login", ({ server }) => {
  registryPasswords.add(String(server));
});
registerMock("docker_registry_logout", ({ server }) => {
  registryPasswords.delete(String(server));
});
registerMock("docker_registry_status", ({ servers }) => (servers as string[]).map((s) => registryPasswords.has(s)));
