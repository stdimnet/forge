import { onMount, Show } from "solid-js";
import { Workbench } from "./shell/Workbench";
import { WelcomeScreen } from "./shell/welcome/WelcomeScreen";
import { openWorkspace, restoreWorkspace, workspace } from "./state/workspace";
import { openFile } from "./state/editor";
import { isTauri } from "./ipc";

export function App() {
  onMount(async () => {
    if (isTauri()) {
      await restoreWorkspace();
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
  return (
    <Show when={workspace()} fallback={<WelcomeScreen />}>
      <Workbench />
    </Show>
  );
}
