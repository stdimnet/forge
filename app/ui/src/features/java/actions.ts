/** Build Project (Ctrl+F9) and Reload All Maven / Gradle Projects. */
import { javaApi } from "@/ipc/java";
import { balloonsFor } from "@/keymap/dispatch";
import { useTemporary } from "@/features/run/configStore";
import { launch } from "@/features/run/runStore";
import { buildView, refreshBuild } from "./store";

const showBalloon = balloonsFor("Java");

/** Compile main and test sources through the build tool (`classes testClasses`, `test-compile`). */
export async function buildProject(): Promise<void> {
  const b = buildView() ?? (await refreshBuild());
  if (!b) {
    showBalloon("Nothing to build", "The project has no Maven or Gradle build in its root.", 5000);
    return;
  }
  const gradle = b.project.tool === "gradle";
  const cfg = useTemporary({
    name: `Build ${b.project.name}`,
    type: gradle ? "gradle" : "maven",
    options: gradle ? { tasks: "classes testClasses" } : { goals: "test-compile" },
  });
  await launch(cfg);
}

/** JDT LS re-imports the build files (after editing build.gradle or pom.xml). */
export async function reloadProject(): Promise<void> {
  try {
    await javaApi.reloadProject();
    await refreshBuild();
    showBalloon("Reloading Maven and Gradle projects", "Eclipse JDT LS re-imports the build; progress shows in the status bar.", 4000);
  } catch (e) {
    showBalloon("Cannot reload the project", String(e).replace(/^Error: /, ""), 8000);
  }
}
