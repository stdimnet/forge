/** Build Project (Ctrl+F9), Reload All Maven / Gradle Projects and running tasks and goals. */
import { javaApi } from "@/ipc/java";
import { getSetting } from "@/state/settings";
import { balloonsFor } from "@/keymap/dispatch";
import { useTemporary } from "@/features/run/configStore";
import { launch } from "@/features/run/runStore";
import type { ProjectModel } from "@/ipc/java";
import { buildModel, buildView, refreshBuild, syncError, syncProject } from "./store";
import { t } from "@/i18n";

const showBalloon = balloonsFor("Java");

/** Compile main and test sources through the build tool (`classes testClasses`, `test-compile`). */
export async function buildProject(): Promise<void> {
  const b = buildView() ?? (await refreshBuild());
  if (!b) {
    showBalloon(t("langs.java.nothingToBuild"), t("langs.java.theProjectHasNoMaven"), 5000);
    return;
  }
  const gradle = b.project.tool === "gradle";
  // Settings | Build Tools | Gradle | Build and run using: IntelliJ IDEA → JDT LS compiles.
  if (gradle && getSetting<string>("java.gradle.buildAndRunUsing", "gradle") === "idea") {
    try {
      const status = await javaApi.buildWorkspace();
      if (status === 1) showBalloon(t("langs.java.buildCompleted"), t("langs.java.eclipseJdtLsCompiledThe"), 3000);
      else showBalloon(t("langs.java.buildCompletedWithErrors"), t("langs.java.seeTheProblemsToolWindow"), 6000);
    } catch (e) {
      showBalloon(t("langs.java.cannotBuildTheProject"), String(e).replace(/^Error: /, ""), 8000);
    }
    return;
  }
  const cfg = useTemporary({
    name: `Build ${b.project.name}`,
    type: gradle ? "gradle" : "maven",
    options: gradle ? { tasks: "classes testClasses" } : { goals: "test-compile" },
  });
  await launch(cfg);
}

/** Reload All Gradle / Maven Projects: the build tool syncs (tasks, dependency trees), then
 * JDT LS re-imports the build files. */
export async function reloadProject(): Promise<void> {
  const m = await syncProject();
  if (m) {
    const n = m.projects.length;
    showBalloon(t("langs.java.syncFinished", { value: m.tool === "gradle" ? "Gradle" : "Maven" }), t("langs.java.projectsSyncedEclipseJdtLs", { count: n }), 4000);
  } else if (syncError()) {
    showBalloon(t("langs.java.syncFailed", { value: buildView()?.project.tool === "maven" ? "Maven" : "Gradle" }), syncError()!, 8000);
  }
}

/** `:app:test` (the root's tasks keep their plain name). */
export const gradleTask = (p: Pick<ProjectModel, "path">, task: string) => (!p.path || p.path === ":" ? task : `${p.path}:${task}`);

/** Run Gradle tasks as a temporary Gradle configuration. */
export function runGradle(tasks: string, name?: string) {
  const root = buildModel()?.projects[0]?.name ?? "project";
  void launch(useTemporary({ name: name ?? `${root} [${tasks}]`, type: "gradle", options: { tasks } }));
}

/** Run Maven goals (in a module) as a temporary Maven configuration. */
export function runMaven(goals: string, module = "") {
  const m = buildModel();
  const project = m?.projects.find((p) => p.dir === module) ?? m?.projects[0];
  void launch(useTemporary({ name: `${project?.name ?? "project"} [${goals}]`, type: "maven", options: module ? { goals, module } : { goals } }));
}

