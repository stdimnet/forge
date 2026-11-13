/**
 * Services providers: features that add their own nodes to the Services tool window next to
 * Docker, Docker Compose and Kubernetes (the Spring Boot run dashboard). A provider owns its
 * node ids, computes their details (in Rust, through its own IPC commands) and handles the
 * toolbar actions on them.
 *
 *   registerServicesProvider({ id: "spring", nodes, owns: (id) => id.startsWith("spring"), details, action });
 */
import { createSignal, type Accessor } from "solid-js";
import type { ServiceAction, ServiceDetails, ServiceNode } from "@/ipc/services";

export interface ServicesProvider {
  id: string;
  /** Root nodes, listed before the backend's (reactive). */
  nodes: Accessor<ServiceNode[]>;
  /** Whether a node id belongs to this provider. */
  owns: (nodeId: string) => boolean;
  details: (nodeId: string) => Promise<ServiceDetails | null>;
  /** Start, Stop, Restart, … on one of its nodes. */
  action: (details: ServiceDetails, action: ServiceAction) => Promise<void>;
  /** Refresh (toolbar button, `Services.Refresh`). */
  refresh?: () => Promise<void>;
}

const [providers, setProviders] = createSignal<ServicesProvider[]>([]);

/** Add (or replace, by id) a provider. */
export function registerServicesProvider(p: ServicesProvider): void {
  setProviders((prev) => [...prev.filter((x) => x.id !== p.id), p]);
}

export { providers };

/** The provider owning a node, if any. */
export const providerFor = (nodeId: string | null | undefined): ServicesProvider | undefined =>
  nodeId ? providers().find((p) => p.owns(nodeId)) : undefined;

/** The root nodes of all providers. */
export const providerNodes = (): ServiceNode[] => providers().flatMap((p) => p.nodes());
