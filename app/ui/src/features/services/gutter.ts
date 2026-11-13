/**
 * ▶ in the gutter of compose files (`services:` runs all services, each service runs itself
 * with `docker compose up`), Dockerfiles (build the image and run a container) and
 * Kubernetes manifests (`kubectl apply -f` / `delete -f` in the selected context, on a
 * remote project's host). The markers come from Rust (`services_gutter`); output shows in
 * the Services tool window.
 */
import { Play, PlayCircle, Trash2 } from "lucide-solid";
import { registerLineMarkerProvider, type LineMarker } from "@/editor/lineMarkers";
import { openMenu } from "@/features/run/PopupMenu";
import { showBalloon } from "@/keymap";
import { servicesApi, type GutterMarker, type ManifestAction } from "@/ipc/services";
import { showToolWindow } from "@/state/layout";
import { kubeSelection, select as selectNode, setTab } from "./store";
import { t } from "@/i18n";

const COMPOSE = /(^|\/)(docker-)?compose(\.[\w.-]+)?\.ya?ml$/;
const DOCKERFILE = /(^|\/)((Docker|Container)file(\.[\w.-]+)?|[\w.-]+\.(docker|container)file)$/i;
const YAML = /\.ya?ml$/i;

/** `kubectl apply -f` / `delete -f`; output in the Kubernetes node's Log tab. */
async function kubectl(path: string, action: ManifestAction): Promise<void> {
  showToolWindow("services");
  selectNode("kube");
  setTab("log");
  try {
    await servicesApi.kubeApply(path, action);
  } catch (err) {
    showBalloon(action === "apply" ? t("tools.services.kubectlApplyFailed") : t("tools.services.kubectlDeleteFailed"), String(err).replace(/^Error: /, ""), 6000);
  }
}

async function run(m: GutterMarker, path: string): Promise<void> {
  showToolWindow("services");
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
    showBalloon(m.kind === "dockerfile" ? t("tools.services.buildFailed") : t("tools.services.dockerComposeUpFailed"), String(err).replace(/^Error: /, ""), 6000);
  }
}

registerLineMarkerProvider({
  appliesTo: (path) => COMPOSE.test(path) || DOCKERFILE.test(path) || YAML.test(path),
  markers: async (path, text): Promise<LineMarker[]> => {
    const marks = await servicesApi.gutter(path, text);
    return marks.map((m) => ({
      line: m.line,
      icon: m.kind === "composeAll" ? PlayCircle : Play,
      title: m.title,
      className: "marker-run",
      onClick: (_view, e) => {
        if (m.kind === "kubeManifest") {
          const k = kubeSelection();
          const where = k?.context ? ` to '${k.context}'${k.host ? ` on ${k.host}` : ""}` : "";
          openMenu({
            x: e.clientX,
            y: e.clientY + 8,
            items: [
              { label: `Apply${where}`, icon: Play, iconColor: "var(--success)", run: () => void kubectl(path, "apply") },
              { label: `Delete from '${k?.context ?? "Kubernetes"}'`, icon: Trash2, run: () => void kubectl(path, "delete") },
            ],
          });
          return;
        }
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
        });
      },
    }));
  },
});
