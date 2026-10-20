/**
 * Services tool window (Alt+8): Docker, Docker Compose and Kubernetes. Replaces the
 * `services` placeholder; also contributes the Docker status bar item, the settings pages
 * and the ▶ gutter icons in compose files and Dockerfiles.
 */
import { Show } from "solid-js";
import { Container, LoaderCircle } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerStatusItem } from "@/features/statusItems";
import { usesProjectType } from "@/state/project";
import { registerCommand } from "@/state/commands";
import { showToolWindow } from "@/state/layout";
import { onFsChanged } from "@/state/fsWatch";
import { servicesApi } from "@/ipc/services";
import { ServicesView } from "./ServicesView";
import { initServices, pullImageDialog, refresh, select, status } from "./store";
import "./settings";
import "./gutter";

registerToolWindow({
  id: "services",
  title: "Services",
  icon: Container,
  anchor: "bottom",
  order: 30,
  shortcut: "Alt+8",
  component: ServicesView,
});

registerCommand({ id: "Services.Refresh", title: "Refresh Services", category: "Services", run: () => void refresh() });
registerCommand({ id: "Docker.PullImage", title: "Pull Image…", category: "Docker", run: pullImageDialog });

/** Docker connection in the status bar; click opens the Services tool window. */
function DockerStatusItem() {
  const label = () => {
    const s = status();
    switch (s?.state) {
      case "connected":
        return `Docker ${s.version ?? ""}`.trim();
      case "connecting":
        return "Docker: connecting…";
      case "failed":
        return "Docker: not connected";
      default:
        return "Docker";
    }
  };
  const title = () => {
    const s = status();
    if (!s) return "Docker";
    if (s.state === "failed") return `Cannot connect to ${s.endpoint || "the Docker daemon"}: ${s.error ?? ""}`;
    return `Docker daemon at ${s.endpoint}`;
  };
  return (
    <Show when={status()}>
      <button
        class="status-item svc-status"
        classList={{ failed: status()?.state === "failed", connected: status()?.state === "connected" }}
        title={title()}
        data-testid="docker-status"
        onClick={() => {
          showToolWindow("bottom", "services");
          select("docker");
        }}
      >
        <Show when={status()?.state !== "connecting"} fallback={<LoaderCircle size={12} class="svc-spin" />}>
          <Container size={12} />
        </Show>
        {label()}
      </button>
    </Show>
  );
}

// Only for projects that use Docker (a Dockerfile or compose file, detected in Rust).
registerStatusItem({ id: "docker", align: "right", order: 70, component: DockerStatusItem, when: () => usesProjectType("docker") });

// Compose files changed on disk (edited, checked out): the backend re-reads them.
const COMPOSE_FILE = /(^|\/)(docker-)?compose(\.[\w.-]+)?\.ya?ml$/;
onFsChanged((e) => {
  if ([...e.changed, ...e.removed].some((p) => COMPOSE_FILE.test(p))) void servicesApi.composeChanged().catch(() => undefined);
});

void initServices();
