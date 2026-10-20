/**
 * ▶ in the gutter of compose files (`services:` runs all services, each service runs itself
 * with `docker compose up`) and Dockerfiles (build the image and run a container). The
 * markers come from Rust (`services_gutter`); output shows in the Services tool window.
 */
import { Play, PlayCircle } from "lucide-solid";
import { registerLineMarkerProvider, type LineMarker } from "@/editor/lineMarkers";
import { openMenu } from "@/features/run/PopupMenu";
import { showBalloon } from "@/keymap";
import { servicesApi, type GutterMarker } from "@/ipc/services";
import { showToolWindow } from "@/state/layout";
import { select as selectNode, setTab } from "./store";

const COMPOSE = /(^|\/)(docker-)?compose(\.[\w.-]+)?\.ya?ml$/;
const DOCKERFILE = /(^|\/)((Docker|Container)file(\.[\w.-]+)?|[\w.-]+\.(docker|container)file)$/i;

async function run(m: GutterMarker, path: string): Promise<void> {
  showToolWindow("bottom", "services");
  try {
    if (m.kind === "dockerfile") {
      selectNode("docker");
      setTab("log");
      await servicesApi.dockerfileRun(path);
    } else {
      // The project node shows the compose output.
      const nodes = await servicesApi.tree();
      const compose = nodes.find((n) => n.id === "compose");
      const project = compose?.children?.find((p) => p.detail && path.endsWith(p.detail));
      if (project) {
        selectNode(project.id);
        setTab("log");
      }
      await servicesApi.composeUp(path, m.service);
    }
  } catch (err) {
    showBalloon(m.kind === "dockerfile" ? "Build failed" : "docker compose up failed", String(err).replace(/^Error: /, ""), 6000);
  }
}

registerLineMarkerProvider({
  appliesTo: (path) => COMPOSE.test(path) || DOCKERFILE.test(path),
  markers: async (path, text): Promise<LineMarker[]> => {
    const marks = await servicesApi.gutter(path, text);
    return marks.map((m) => ({
      line: m.line,
      icon: m.kind === "composeAll" ? PlayCircle : Play,
      title: m.title,
      className: "marker-run",
      onClick: (_view, e) =>
        openMenu({
          x: e.clientX,
          y: e.clientY + 8,
          items: [
            {
              label: m.kind === "dockerfile" ? "Run on 'Docker'" : m.kind === "composeAll" ? "Run all services" : `Run '${m.service}'`,
              icon: Play,
              iconColor: "var(--success)",
              run: () => void run(m, path),
            },
          ],
        }),
    }));
  },
});
