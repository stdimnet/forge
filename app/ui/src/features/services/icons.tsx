/** Node icons of the Services tree with a status badge (running, stopped, error, busy). */
import { Show, type JSX } from "solid-js";
import { Dynamic } from "solid-js/web";
import {
  AppWindow,
  Box,
  Boxes,
  CircleAlert,
  Container,
  Cuboid,
  Folder,
  FolderKanban,
  HardDrive,
  Layers,
  Leaf,
  LoaderCircle,
  Network,
  Orbit,
  Package,
  Rocket,
  Server,
  ShipWheel,
  Waypoints,
} from "lucide-solid";
import type { NodeIcon, NodeStatus } from "@/ipc/services";

type IconComp = (p: { size?: number; "stroke-width"?: number; class?: string }) => JSX.Element;

export const NODE_ICONS: Record<NodeIcon, IconComp> = {
  docker: Container,
  folder: Folder,
  container: Package,
  image: Layers,
  network: Network,
  volume: HardDrive,
  compose: Boxes,
  service: Box,
  kubernetes: ShipWheel,
  context: Server,
  namespace: FolderKanban,
  pod: Cuboid,
  deployment: Rocket,
  kubeService: Waypoints,
  springBoot: Leaf,
  springApp: Leaf,
  aspire: Orbit,
  dotnetProject: AppWindow,
  error: CircleAlert,
};

export function NodeIconView(props: { icon: NodeIcon; status?: NodeStatus; size?: number }) {
  return (
    <span class={`svc-icon svc-ic-${props.icon}`}>
      <Dynamic component={NODE_ICONS[props.icon]} size={props.size ?? 14} stroke-width={1.8} />
      <Show when={props.status}>
        {(s) => (
          <Show when={s() !== "busy"} fallback={<LoaderCircle size={9} class="svc-badge-spin" />}>
            <span class={`svc-badge ${s()}`} />
          </Show>
        )}
      </Show>
    </span>
  );
}

