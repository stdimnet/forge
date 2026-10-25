/**
 * Dev Containers and WSL projects: the "Create Dev Container and Mount Sources" dialog with
 * its build log, the devcontainer.json gutter icon, Welcome screen > Remote Development >
 * Dev Containers / WSL, opening `\\wsl$\…` folders in their distribution and Terminal › WSL.
 * The backend (`crates/forge-devenv`) starts the Forge agent in the container or the
 * distribution; the project then works like a remote project.
 */
import { createResource, createRoot, createSignal } from "solid-js";
import { render } from "solid-js/web";
import { registerCommand } from "@/state/commands";
import { registerPathHandler, workspace } from "@/state/workspace";
import { registerShellProfiles } from "@/features/terminal/store";
import { wslApi } from "@/ipc/devenv";
import { isDialogWindow } from "@/shell/window/mode";
import { balloonsFor } from "@/keymap";
import { DevContainerDialog } from "./DevContainerDialog";
import { setRecentDevEnvOpener } from "@/features/remote/store";
import { createDevContainer, openRecentDevEnv, openWslPath } from "./store";
import "./gutter";
import "./devenv.css";

const showBalloon = balloonsFor("Dev Containers");

registerCommand({
  id: "DevContainers.Create",
  title: "Create Dev Container and Mount Sources…",
  category: "File",
  run: () => {
    if (!workspace() || workspace()?.remote)
      showBalloon("Dev Containers", "Open a local project with a devcontainer.json to create its dev container.");
    else createDevContainer();
  },
});

registerPathHandler(openWslPath);
setRecentDevEnvOpener(openRecentDevEnv);

// Terminal "+" › WSL: a shell in each distribution, in the project folder (local projects).
createRoot(() => {
  const [asked, setAsked] = createSignal(false);
  const [info] = createResource(asked, () => wslApi.distributions().catch(() => null));
  const [cwd] = createResource(
    () => (asked() && workspace() && !workspace()?.remote ? workspace()!.root : null),
    (root) => wslApi.mapPath(root).catch(() => null),
  );
  registerShellProfiles(() => {
    if (!asked()) queueMicrotask(() => setAsked(true));
    if (workspace()?.remote || !info()?.available) return [];
    const dir = cwd();
    return info()!.distros.map((d) => ({
      label: `WSL: ${d.name}`,
      command: {
        program: "wsl.exe",
        args: ["-d", d.name, ...(dir && !dir.distro ? ["--cd", dir.path] : [])],
        title: d.name,
      },
    }));
  });
});

if (typeof document !== "undefined" && !isDialogWindow()) {
  const host = document.createElement("div");
  host.id = "devenv-overlays";
  document.body.appendChild(host);
  render(() => <DevContainerDialog />, host);
}
