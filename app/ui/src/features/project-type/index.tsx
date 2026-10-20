/**
 * Project type: shows the detected framework in the status bar and announces newly detected
 * frameworks once per project ("Laravel project detected — Laravel support enabled"), the
 * way PHPStorm's framework detection does.
 */
import { Show } from "solid-js";
import { registerStatusItem, visibleStatusItems } from "@/features/statusItems";
import { settingsPages } from "@/features/settingsPages";
import { openSettings } from "@/features/settings/dialogState";
import { registerCommand } from "@/state/commands";
import { getSetting, saveSetting, settingsState } from "@/state/settings";
import { frameworkLabel, isPluginActive, onProjectDetected, projectInfo, refreshProjectInfo, trackProjectInfo } from "@/state/project";
import { showBalloon } from "@/keymap";
import { ProjectTypeIcon } from "@/shell/ProjectTypeIcon";
import type { ProjectInfo } from "@/ipc/project";
import { announcement } from "./announce";
import { isDialogWindow } from "@/shell/window/mode";

/** User setting holding `<root>#<framework>` pairs already announced. */
export const ANNOUNCED_KEY = "project.announcedFrameworks";

/**
 * Frameworks whose plugin shows its own status bar widget (e.g. "Symfony 7.2 · dev");
 * the generic widget then stays hidden for them.
 */
const claimed = new Set<string>();

export function claimFrameworkStatus(frameworkId: string): void {
  claimed.add(frameworkId);
}

function ProjectTypeStatus() {
  const primary = () => {
    const p = projectInfo()?.primary;
    return p && !(claimed.has(p.id) && isPluginActive(`forge.${p.id}`)) ? p : undefined;
  };
  // A framework plugin may show its own widget (`framework.<id>`, e.g. Laravel's).
  const replaced = () => visibleStatusItems("right").some((i) => i.id === `framework.${primary()?.id}`);
  const others = () =>
    (projectInfo()?.frameworks ?? [])
      .filter((f) => f.id !== primary()?.id)
      .map(frameworkLabel)
      .join(", ");
  return (
    <Show when={replaced() ? undefined : primary()}>
      {(p) => (
        <button
          class="status-item status-project-type"
          title={`Detected: ${frameworkLabel(p())}${others() ? ` (also ${others()})` : ""}. Click to configure plugins`}
          onClick={() => openSettings("plugins")}
        >
          <ProjectTypeIcon type={p().id} size={14} />
          {frameworkLabel(p())}
        </button>
      )}
    </Show>
  );
}

registerStatusItem({ id: "project.type", align: "right", order: 80, component: ProjectTypeStatus });

const shownThisSession = new Set<string>();

function announce(info: ProjectInfo, root: string, attempt = 0): void {
  // Plugin enablement comes with the settings; wait for them on startup.
  if (!settingsState.loaded) {
    if (attempt < 50) setTimeout(() => announce(info, root, attempt + 1), 200);
    return;
  }
  const announced = getSetting<string[]>(ANNOUNCED_KEY, []) ?? [];
  const a = announcement(info, root, settingsState.plugins, [...announced, ...shownThisSession]);
  if (!a) return;
  shownThisSession.add(a.key);
  void saveSetting(ANNOUNCED_KEY, [...announced, a.key].slice(-200), "user").catch(() => {});
  const page = a.settingsPages.find((id) => settingsPages().some((p) => p.id === id)) ?? "plugins";
  showBalloon(a.title, a.detail, 12000, [{ label: "Configure", run: () => openSettings(page) }]);
}

// The "support enabled" notification shows in the IDE window only.
if (!isDialogWindow()) onProjectDetected(announce);
trackProjectInfo();

registerCommand({ id: "project.redetect", title: "Detect Project Type", category: "File", run: refreshProjectInfo });
