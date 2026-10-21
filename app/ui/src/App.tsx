import { onMount, Show } from "solid-js";
import { Workbench } from "./shell/Workbench";
import { WelcomeScreen } from "./shell/welcome/WelcomeScreen";
import { openWorkspace, restoreWorkspace, workspace } from "./state/workspace";
import { openFile } from "./state/editor";
import { isTauri } from "./ipc";
import { drawsControls, trackMaximized } from "./shell/window/chrome";
import { ResizeEdges, WindowTitleBar } from "./shell/window/WindowFrame";
import { reportWorkbenchReady } from "./features/updates";
// The project's tool window layout and editor tabs are restored and saved (main window only).
import "./state/layoutPersistence";

export function App() {
  onMount(async () => {
    trackMaximized();
    if (isTauri()) {
      await restoreWorkspace();
      // The window is shown (and the splash closed) once the first screen has rendered.
      // The window is still hidden here, so no animation frame would ever run: use a task.
      setTimeout(() => void reportWorkbenchReady(), 0);
      return;
    }
    // In browser mode open the demo project so the IDE is explorable without a backend,
    // unless `?welcome` asks for the Welcome screen.
    if (new URLSearchParams(location.search).has("welcome")) return;
    await openWorkspace("demo");
    const INITIAL: Record<string, string> = {
      symfony: "src/Controller/OrderController.php",
      lua: "main.lua",
      react: "src/App.tsx",
      vue: "src/views/DashboardView.vue",
      angular: "src/app/users/user-card.component.ts",
    };
    await openFile(INITIAL[new URLSearchParams(location.search).get("project") ?? ""] ?? "app/Models/User.php");
  });
  const welcome = () => (
    <Show when={drawsControls()} fallback={<WelcomeScreen />}>
      <div class="welcome-window">
        <WindowTitleBar title="Welcome to Forge" />
        <WelcomeScreen />
      </div>
    </Show>
  );
  return (
    <>
      <Show when={workspace()} fallback={welcome()}>
        <Workbench />
      </Show>
      <ResizeEdges />
    </>
  );
}
