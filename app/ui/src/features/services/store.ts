/**
 * Services tool window state: the tree computed by the backend, expansion and selection,
 * the Docker connection for the status bar and the consoles (Log tabs) fed by
 * `services://event`.
 */
import { createSignal, type Accessor, type Setter } from "solid-js";
import { balloonsFor } from "@/keymap";
import { setInputDialog } from "@/features/git/overlays";
import { openTerminalWith } from "@/features/terminal";
import { showToolWindow } from "@/state/layout";
import type { ConsoleLine } from "@/ipc/run";
import {
  servicesApi,
  type DetailsTab,
  type ServiceAction,
  type ServiceDetails,
  type ServiceNode,
  type ServicesEvent,
} from "@/ipc/services";

const showBalloon = balloonsFor("Services");

const [nodes, setNodes] = createSignal<ServiceNode[]>([]);
const [expanded, setExpanded] = createSignal<ReadonlySet<string>>(new Set());
const [selected, setSelected] = createSignal<string | null>(null);
const [tab, setTab] = createSignal<DetailsTab>("dashboard");
const [version, setVersion] = createSignal(0);
const [status, setStatus] = createSignal<Extract<ServicesEvent, { type: "status" }> | null>(null);
const [loading, setLoading] = createSignal(new Set<string>());
/** Picked container per pod node (Log, Exec, Attached Console); default: the pod's default. */
const [containers, setContainers] = createSignal<Record<string, string>>({});
/** The pod whose Forward Ports dialog is open. */
const [forwardDialog, setForwardDialog] = createSignal<ServiceDetails | null>(null);

export { nodes, expanded, selected, tab, setTab, version, status, loading, forwardDialog, setForwardDialog };

/** The container picked for a pod node, else its default container. */
export function containerOf(d: ServiceDetails): string | undefined {
  const list = d.containers ?? [];
  const picked = containers()[d.nodeId];
  if (picked && list.some((c) => c.name === picked)) return picked;
  return list.find((c) => c.default)?.name ?? list.find((c) => !c.init)?.name;
}

export function pickContainer(nodeId: string, name: string): void {
  setContainers((m) => ({ ...m, [nodeId]: name }));
}

const seen = new Set<string>();

function applyNodes(list: ServiceNode[]): void {
  const open = new Set(expanded());
  const walk = (ns: ServiceNode[]) => {
    for (const n of ns) {
      // Lazy nodes get their initial expansion once their children are loaded.
      if (!seen.has(n.id) && !n.lazy) {
        seen.add(n.id);
        if (n.expanded) open.add(n.id);
      }
      if (n.children) walk(n.children);
    }
  };
  walk(list);
  setExpanded(open);
  setNodes(list);
  setVersion((v) => v + 1);
  if (!selected() && list.length) setSelected(list[0].id);
}

export function findNode(id: string | null, list = nodes()): ServiceNode | undefined {
  if (!id) return undefined;
  for (const n of list) {
    if (n.id === id) return n;
    const c = n.children && findNode(id, n.children);
    if (c) return c;
  }
  return undefined;
}

// ---- consoles ---------------------------------------------------------------------------

export interface ConsoleState {
  lines: ConsoleLine[];
  version: Accessor<number>;
  bump: Setter<number>;
  ended: Accessor<boolean>;
  setEnded: Setter<boolean>;
  loaded: boolean;
}

const consoles = new Map<string, ConsoleState>();

export function consoleFor(key: string): ConsoleState {
  let c = consoles.get(key);
  if (!c) {
    const [v, bump] = createSignal(0);
    const [ended, setEnded] = createSignal(false);
    c = { lines: [], version: v, bump, ended, setEnded, loaded: false };
    consoles.set(key, c);
  }
  if (!c.loaded) {
    c.loaded = true;
    const state = c;
    void servicesApi
      .console(key)
      .then((lines) => {
        if (!state.lines.length && lines.length) {
          state.lines.push(...lines);
          state.bump((n) => n + 1);
        }
      })
      .catch(() => undefined);
  }
  return c;
}

function onEvent(e: ServicesEvent): void {
  switch (e.type) {
    case "tree":
      applyNodes(e.nodes);
      break;
    case "status":
      setStatus(e);
      break;
    case "console": {
      const c = consoleFor(e.key);
      if (e.reset) c.lines.length = 0;
      if (e.delta.replaceLast && e.delta.lines.length) c.lines.pop();
      c.lines.push(...e.delta.lines);
      if (c.lines.length > 5000) c.lines.splice(0, c.lines.length - 5000);
      c.setEnded(false);
      c.bump((n) => n + 1);
      break;
    }
    case "ended":
      consoleFor(e.key).setEnded(true);
      break;
    case "error":
      showBalloon(e.message);
      break;
  }
}

let started: Promise<void> | null = null;

/** Subscribe and load the tree (once). */
export function initServices(): Promise<void> {
  started ??= (async () => {
    await servicesApi.onEvent(onEvent);
    try {
      applyNodes(await servicesApi.tree());
      setStatus(await servicesApi.status());
    } catch (err) {
      console.warn("services not available", err);
    }
  })();
  return started;
}

export async function refresh(): Promise<void> {
  try {
    applyNodes(await servicesApi.refresh());
  } catch (err) {
    showBalloon("Refresh failed", String(err));
  }
}

// ---- tree interaction ---------------------------------------------------------------------

export function select(id: string): void {
  setSelected(id);
}

export async function setOpen(node: ServiceNode, open: boolean): Promise<void> {
  const next = new Set(expanded());
  if (!open) {
    next.delete(node.id);
    setExpanded(next);
    return;
  }
  next.add(node.id);
  setExpanded(next);
  if (node.lazy) {
    setLoading((s) => new Set(s).add(node.id));
    try {
      applyNodes(await servicesApi.expand(node.id));
    } catch (err) {
      showBalloon(`Cannot load ${node.label}`, String(err));
    } finally {
      setLoading((s) => {
        const n = new Set(s);
        n.delete(node.id);
        return n;
      });
    }
  }
}

export function collapseAll(): void {
  setExpanded(new Set<string>());
}

export function expandAll(): void {
  const open = new Set<string>();
  const walk = (ns: ServiceNode[]) =>
    ns.forEach((n) => {
      if (n.children?.length) {
        open.add(n.id);
        walk(n.children);
      }
    });
  walk(nodes());
  setExpanded(open);
}

// ---- actions ----------------------------------------------------------------------------

const CONFIRM: Partial<Record<ServiceAction, (d: ServiceDetails) => string>> = {
  remove: (d) =>
    d.target.kind === "pod" ? `Delete pod '${d.title}'?` : d.target.kind === "container" ? `Delete container '${d.title}'? It will be stopped first.` : `Delete '${d.title}'?`,
  down: (d) =>
    d.target.kind === "composeProject"
      ? `Stop and remove the containers and networks of '${d.title}' (docker compose down)?`
      : `Stop and remove the containers of service '${d.title}'?`,
};

/** Toolbar action on the details' node; asks before destructive ones. */
export async function runAction(d: ServiceDetails, action: ServiceAction): Promise<void> {
  if (action === "logs") {
    setTab("log");
    return;
  }
  if (action === "exec") {
    await openExec(d.nodeId, containerOf(d));
    return;
  }
  if (action === "forwardPorts") {
    setForwardDialog(d);
    return;
  }
  if (action === "pull" && d.target.kind === "dockerGroup") {
    pullImageDialog();
    return;
  }
  const confirm = CONFIRM[action];
  const go = async () => {
    // Compose output goes to the project's Log tab.
    if (d.target.kind === "composeProject" || d.target.kind === "composeService") {
      const project = `compose/${encodeURIComponent(d.target.project)}`;
      if (action !== "remove" && action !== "down") {
        setSelected(project);
        setTab("log");
      }
    } else if (action === "pull") {
      setSelected("docker");
      setTab("log");
    }
    try {
      await servicesApi.action(d.nodeId, action);
    } catch (err) {
      showBalloon(`${action[0].toUpperCase()}${action.slice(1)} failed`, String(err).replace(/^Error: /, ""), 6000);
    }
  };
  if (!confirm) return go();
  setInputDialog({
    title: action === "down" ? "Docker Compose Down" : "Delete",
    label: confirm(d),
    value: "",
    okLabel: action === "down" ? "Down" : "Delete",
    noInput: true,
    onOk: go,
  });
}

/** Forward Ports: start forwards for `spec` (`8080:80, 9000`); throws the backend's error. */
export async function forwardPorts(nodeId: string, spec: string): Promise<void> {
  const started = await servicesApi.portForwardStart(nodeId, spec);
  const list = started.map((f) => `${f.address}:${f.localPort} → ${f.remotePort}`).join(", ");
  showBalloon(`Forwarding ${list}`, `Pod ${started[0]?.pod ?? ""}`);
}

export async function stopForward(id: number): Promise<void> {
  try {
    await servicesApi.portForwardStop(id);
  } catch (err) {
    showBalloon("Cannot stop the port forward", String(err).replace(/^Error: /, ""));
  }
}

export function pullImageDialog(): void {
  setInputDialog({
    title: "Pull Image",
    label: "Image to pull (repository[:tag], e.g. redis:7-alpine):",
    value: "",
    okLabel: "Pull",
    onOk: async (image) => {
      showToolWindow("bottom", "services");
      setSelected("docker");
      setTab("log");
      try {
        await servicesApi.pull(image);
      } catch (err) {
        showBalloon("Pull failed", String(err).replace(/^Error: /, ""), 6000);
      }
    },
  });
}

/** Exec: a terminal tab with a shell in the container or pod (`container` of a pod). */
export async function openExec(nodeId: string, container?: string): Promise<void> {
  try {
    const cmd = await servicesApi.terminal(nodeId, false, container);
    openTerminalWith({ title: cmd.title, program: cmd.program, args: cmd.args, env: cmd.env, local: !cmd.onHost });
  } catch (err) {
    showBalloon("Cannot open a terminal", String(err).replace(/^Error: /, ""));
  }
}
