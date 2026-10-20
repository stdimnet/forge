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
    const symfony = new URLSearchParams(location.search).get("project") === "symfony";
    await openFile(symfony ? "src/Controller/OrderController.php" : "app/Models/User.php");
  });
  return (
    <Show when={workspace()} fallback={<WelcomeScreen />}>
      <Workbench />
    </Show>
  );
}
