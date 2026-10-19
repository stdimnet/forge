import { onMount } from "solid-js";
import { Workbench } from "./shell/Workbench";
import { openWorkspace } from "./state/workspace";
import { openFile } from "./state/editor";
import { isTauri } from "./ipc";

export function App() {
  onMount(async () => {
    // In browser mode open the demo project so the IDE is explorable without a backend.
    if (!isTauri()) {
      await openWorkspace("demo");
      await openFile("app/Models/User.php");
    }
  });
  return <Workbench />;
}
