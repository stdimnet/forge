/**
 * Services tool window backend (Rust: `crates/forge-containers`, `commands/services.rs`):
 * the Docker / Docker Compose / Kubernetes tree, details, actions, logs and terminals. All
 * models are computed in Rust; the UI renders them.
 */
import { invoke, listen } from "./index";
import type { ConsoleLine } from "./run";

export type NodeIcon =
  | "docker"
  | "folder"
  | "container"
  | "image"
  | "network"
  | "volume"
  | "compose"
  | "service"
  | "kubernetes"
  | "context"
  | "namespace"
  | "pod"
  | "deployment"
  | "kubeService"
  | "error";

export type NodeStatus = "running" | "stopped" | "paused" | "pending" | "error" | "busy";

export interface ServiceNode {
  id: string;
  label: string;
  detail?: string;
  icon: NodeIcon;
  status?: NodeStatus;
  children?: ServiceNode[];
  /** Children load on expand (`services_expand`). */
  lazy?: boolean;
  /** Expanded when first shown. */
  expanded?: boolean;
  muted?: boolean;
}

export type ServiceAction =
  | "start"
  | "stop"
  | "restart"
  | "remove"
  | "logs"
  | "exec"
  | "pull"
  | "up"
  | "down"
  | "connect"
  | "refresh"
  | "forwardPorts";

export type DetailsTab = "dashboard" | "log" | "console" | "inspection";

export interface DashboardRow {
  label: string;
  value: string;
  url?: string;
  status?: NodeStatus;
}

export interface DashboardTable {
  columns: string[];
  rows: string[][];
  status?: (NodeStatus | null)[];
}

export interface DashboardSection {
  title: string;
  rows?: DashboardRow[];
  table?: DashboardTable;
}

export type Target = { kind: string; [key: string]: string };

/** A container of a pod in the Log / Exec / Attached Console picker. */
export interface ContainerChoice {
  name: string;
  /** Init containers only have logs. */
  init?: boolean;
  /** The container kubectl uses without `-c`. */
  default?: boolean;
}

/** A port a pod's container declares. */
export interface ContainerPort {
  container: string;
  port: number;
  name?: string;
  protocol: string;
}

/** An active Kubernetes port forward (`127.0.0.1:<localPort>` → pod port). */
export interface PortForward {
  id: number;
  nodeId: string;
  context: string;
  namespace: string;
  pod: string;
  address: string;
  localPort: number;
  remotePort: number;
  connections: number;
  error?: string;
}

/** A Docker CLI context (`docker context ls`). */
export interface DockerContext {
  name: string;
  description?: string;
  host: string;
  skipTlsVerify: boolean;
  tlsDir?: string;
  current: boolean;
}

export interface ServiceDetails {
  nodeId: string;
  target: Target;
  title: string;
  icon: NodeIcon;
  status?: NodeStatus;
  subtitle?: string;
  tabs: DetailsTab[];
  sections: DashboardSection[];
  actions: ServiceAction[];
  message?: string;
  /** Pods: containers for the picker. */
  containers?: ContainerChoice[];
  /** Active port forwards of the pod (all of them on the Kubernetes nodes). */
  portForwards?: PortForward[];
  /** Pods: declared container ports. */
  ports?: ContainerPort[];
}

export interface TerminalCommand {
  title: string;
  program: string;
  args: string[];
  env: [string, string][];
}

export interface GutterMarker {
  /** 0-based. */
  line: number;
  kind: "composeAll" | "composeService" | "dockerfile";
  title: string;
  service?: string;
}

export interface DockerConnectionSettings {
  connection: string;
  socketPath: string;
  tcpUrl: string;
  tls: boolean;
  certPath: string;
  sshUrl: string;
  sshSocket: string;
  context: string;
}

export type ConnectionState = "disconnected" | "connecting" | "connected" | "failed";

export type ServicesEvent =
  | { type: "tree"; nodes: ServiceNode[] }
  | { type: "console"; key: string; delta: { replaceLast: boolean; lines: ConsoleLine[] }; reset?: boolean }
  | { type: "ended"; key: string }
  | { type: "status"; state: ConnectionState; endpoint: string; error?: string | null; version?: string | null }
  | { type: "error"; message: string };

export const SERVICES_EVENT = "services://event";

export const servicesApi = {
  tree: () => invoke<ServiceNode[]>("services_tree"),
  refresh: () => invoke<ServiceNode[]>("services_refresh"),
  composeChanged: () => invoke<void>("services_compose_changed"),
  expand: (nodeId: string) => invoke<ServiceNode[]>("services_expand", { nodeId }),
  details: (nodeId: string) => invoke<ServiceDetails | null>("services_details", { nodeId }),
  action: (nodeId: string, action: ServiceAction) => invoke<void>("services_action", { nodeId, action }),
  inspect: (nodeId: string) => invoke<string>("services_inspect", { nodeId }),
  console: (key: string) => invoke<ConsoleLine[]>("services_console", { key }),
  /** `container` picks a pod's container (default: the pod's default container). */
  logsStart: (nodeId: string, container?: string) => invoke<string>("services_logs_start", { nodeId, container }),
  logsStop: (nodeId: string) => invoke<void>("services_logs_stop", { nodeId }),
  terminal: (nodeId: string, attach: boolean, container?: string) => invoke<TerminalCommand>("services_terminal", { nodeId, attach, container }),
  /** Forward Ports: `spec` is `local:pod` pairs (`8080:80, 9000`, `:80` for any free local port). */
  portForwardStart: (nodeId: string, spec: string) => invoke<PortForward[]>("services_port_forward_start", { nodeId, spec }),
  portForwardStop: (id: number) => invoke<void>("services_port_forward_stop", { id }),
  portForwards: () => invoke<PortForward[]>("services_port_forwards"),
  dockerContexts: () => invoke<DockerContext[]>("docker_contexts"),
  pull: (image: string) => invoke<void>("services_pull", { image }),
  composeUp: (path: string, service?: string) => invoke<void>("services_compose_up", { path, service }),
  dockerfileRun: (path: string) => invoke<void>("services_dockerfile_run", { path }),
  gutter: (path: string, content: string) => invoke<GutterMarker[]>("services_gutter", { path, content }),
  status: () => invoke<Extract<ServicesEvent, { type: "status" }>>("services_status"),
  /** With the entered (unsaved) connection settings; without them the applied ones. */
  testConnection: (settings?: DockerConnectionSettings) => invoke<string>("docker_test_connection", { settings }),
  registryLogin: (server: string, password: string) => invoke<void>("docker_registry_login", { server, password }),
  registryLogout: (server: string) => invoke<void>("docker_registry_logout", { server }),
  registryStatus: (servers: string[]) => invoke<boolean[]>("docker_registry_status", { servers }),
  onEvent: (cb: (e: ServicesEvent) => void) => listen<ServicesEvent>(SERVICES_EVENT, cb),
};

/** Console key of a node's log stream / of its compose and build output. */
export const logKey = (nodeId: string) => `log:${nodeId}`;
export const runKey = (nodeId: string) => `run:${nodeId}`;
