/**
 * JavaForge in the UI, in the style of IntelliJ IDEA. Everything is computed in Rust
 * (`commands/java.rs`, `packs/javaforge/*`); code intelligence comes from Eclipse JDT LS
 * through the shared LSP client (completion, navigation, diagnostics, rename, code actions,
 * organize imports, formatting). This module plugs JavaForge into the contribution points:
 *
 * - Settings: Build, Execution, Deployment | JDKs and Build Tools | Maven / Gradle;
 *   Languages & Frameworks | Java (JDT LS, debugger);
 * - run configuration types (`runTypes.tsx`), the Java debugger (`debug.ts`);
 * - gutter ▶ on `main` methods and Run Context Configuration (Ctrl+Shift+F10);
 * - Build Project (Ctrl+F9), Reload All Maven and Gradle Projects, Download JDK…;
 * - the Gradle / Maven tool windows (tasks, dependencies, sync, offline mode, profiles, Skip
 *   Tests, Execute Gradle Task / Maven Goal…), the Gradle wrapper on the Gradle settings page;
 * - completion and the newer-version inspection in build scripts and pom.xml (`buildScripts.ts`);
 * - the JDT LS install offer and its progress in the status bar.
 */
import { createEffect, createRoot, on, Show } from "solid-js";
import { Bug, Boxes, Hammer, LoaderCircle, Play, Settings2 } from "lucide-solid";
import { isDialogWindow } from "@/shell/window/mode";
import { javaApi, type MainMethod } from "@/ipc/java";
import { registerLineMarkerProvider } from "@/editor/lineMarkers";
import { shortcutLabel } from "@/keymap";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { openSettings } from "@/features/settings/dialogState";
import { registerToolWindow } from "@/features/registry";
import { registerStatusItem } from "@/features/statusItems";
import { action, registerMainMenuEntries } from "@/features/mainMenu/model";
import { useTemporary } from "@/features/run/configStore";
import { debugConfiguration, launch } from "@/features/run/runStore";
import { openMenu } from "@/features/run/PopupMenu";
import { openRunConfigurations } from "@/features/run/dialogState";
import { commands, registerCommand } from "@/state/commands";
import { activeTab } from "@/state/editor";
import { onProjectDetected } from "@/state/project";
import { workspace } from "@/state/workspace";
import { JdksPage, openDownloadForm } from "./JdksPage";
import { JavaPage } from "./JavaPage";
import { GradleToolWindow, MavenToolWindow } from "./BuildToolWindow";
import { GradlePage } from "./GradlePage";
import { openExecuteDialog } from "./ExecuteDialog";
import { buildScriptEditor } from "./buildScripts";
import { registerEditorExtension } from "@/editor/extensions";
import { buildProject, reloadProject } from "./actions";
import { checkJavaSetup, installLanguageServer } from "./install";
import { active, activeInstall, buildView, refreshBuild, refreshJdks, refreshModel } from "./store";
import { APPLICATION } from "./runTypes";
import "./debug";
import "./java.css";
import { t } from "@/i18n";

// ---- Settings ---------------------------------------------------------------------------

registerSettingsPage({
  id: "build.jdks",
  title: t("settings.page.build.jdks"),
  group: "Build, Execution, Deployment",
  order: 1,
  keywords: ["jdk", "sdk", "java home", "JAVA_HOME", "project jdk", "language level", "temurin", "adoptium", "download jdk", "sdkman", "asdf", "jabba", "toolchain"],
  component: JdksPage,
});

registerSettingsPage({
  id: "build.tools",
  title: t("settings.page.build.tools"),
  group: "Build, Execution, Deployment",
  order: 2,
  keywords: ["maven", "gradle", "build tools", "maven central", "dependency completion"],
  component: () => <SchemaPage pageId="build.tools" />,
});

registerSettingsPage({
  id: "build.tools.gradle",
  title: t("settings.page.build.tools.gradle"),
  group: "Build, Execution, Deployment",
  parent: "build.tools",
  order: 0,
  keywords: ["gradle", "gradlew", "wrapper", "gradle jvm", "offline", "distributionSha256Sum", "update wrapper", "build and run using", "run tests using", "junit runner"],
  component: GradlePage,
});

registerSettingsPage({
  id: "build.tools.maven",
  title: t("settings.page.build.tools.maven"),
  group: "Build, Execution, Deployment",
  parent: "build.tools",
  order: 1,
  keywords: ["maven", "mvnw", "wrapper", "maven home", "offline", "sources", "skip tests", "profiles"],
  component: () => <SchemaPage pageId="build.tools.maven" />,
});

registerSettingsPage({
  id: "lang.java",
  title: t("settings.page.lang.java"),
  group: "Languages & Frameworks",
  order: 25,
  keywords: ["java", "jdt", "jdtls", "eclipse", "language server", "java-debug", "hot code replace", "hotswap"],
  component: JavaPage,
});

// ---- Commands ---------------------------------------------------------------------------

registerCommand({ id: "CompileDirty", title: "Build Project", category: "Build", run: () => void buildProject() });
registerCommand({ id: "java.reloadProject", title: "Reload All Maven and Gradle Projects", category: "Java", run: () => void reloadProject() });
registerCommand({ id: "java.syncProject", title: "Reload All Gradle and Maven Projects (Sync)", category: "Java", run: () => void reloadProject() });
registerCommand({ id: "java.executeTask", title: "Execute Gradle Task / Maven Goal…", category: "Java", run: () => openExecuteDialog() });
registerCommand({ id: "java.updateWrapper", title: "Update Gradle Wrapper…", category: "Java", run: () => openSettings("build.tools.gradle") });
registerCommand({
  id: "java.downloadJdk",
  title: "Download JDK…",
  category: "Java",
  run: () => {
    openDownloadForm();
    openSettings("build.jdks");
  },
});
registerCommand({ id: "java.installLanguageServer", title: "Install Eclipse JDT Language Server", category: "Java", run: () => void installLanguageServer() });

registerMainMenuEntries(
  "_Build",
  [action("CompileDirty", "_Build Project"), action("java.reloadProject", "_Reload All Maven and Gradle Projects"), action("java.executeTask", "E_xecute Gradle Task / Maven Goal…")],
  { when: active },
);

// ---- Build scripts in the editor ----------------------------------------------------------

registerEditorExtension(({ path }) => buildScriptEditor(path));

// ---- Tool windows -----------------------------------------------------------------------

registerToolWindow({
  id: "gradle",
  title: t("toolwindow.gradle.title"),
  icon: Boxes,
  anchor: "right",
  order: 40,
  component: GradleToolWindow,
  when: () => active() && buildView()?.project.tool === "gradle",
});

registerToolWindow({
  id: "maven",
  title: t("toolwindow.maven.title"),
  icon: Hammer,
  anchor: "right",
  order: 40,
  component: MavenToolWindow,
  when: () => active() && buildView()?.project.tool === "maven",
});

// ---- Status bar: JDT LS / JDK installs ---------------------------------------------------

registerStatusItem({
  id: "java.install",
  align: "right",
  order: 5,
  when: () => !!activeInstall(),
  component: () => (
    <Show when={activeInstall()}>
      {(i) => (
        <span class="status-item ls-install-status" title={i().label}>
          <LoaderCircle size={13} class="lsp-spin" />
          <span class="ls-install-status-name">{i().id === "jdk" ? "Downloading JDK:" : "Installing Eclipse JDT LS:"}</span>
          <span class="ls-install-status-text">{i().total ? `${i().label} · ${Math.floor((i().done / i().total!) * 100)}%` : i().label}</span>
          <span class="ls-install-bar" classList={{ indeterminate: !i().total }}>
            <span style={{ width: `${Math.round((i().total ? i().done / i().total! : 0.35) * 100)}%` }} />
          </span>
        </span>
      )}
    </Show>
  ),
});

// ---- Project open -----------------------------------------------------------------------

if (typeof document !== "undefined" && !isDialogWindow()) {
  onProjectDetected(() => {
    if (!active()) return;
    void refreshJdks();
    void refreshBuild();
    void refreshModel();
    void checkJavaSetup();
  });
  createRoot(() => {
    createEffect(
      on(
        () => !!workspace() && active(),
        (on) => {
          if (on) {
            void refreshBuild();
            void refreshModel();
          }
        },
      ),
    );
  });
}

// ---- Run gutter on main methods ---------------------------------------------------------

/** The temporary Application configuration of a `main` method. */
function mainConfiguration(m: MainMethod) {
  return useTemporary({ name: m.name, type: APPLICATION, options: { mainClass: m.class } });
}

registerLineMarkerProvider({
  appliesTo: (path) => path.endsWith(".java"),
  watch: active,
  markers: async (path, text) => {
    if (!active()) return [];
    const mains = await javaApi.mainMethods(path, text).catch(() => [] as MainMethod[]);
    return mains.map((m) => ({
      line: m.line,
      icon: Play,
      title: `Run '${m.name}.main()'`,
      className: "marker-run",
      onClick: (_view, e) =>
        openMenu({
          x: e.clientX,
          y: e.clientY + 6,
          items: [
            { label: `Run '${m.name}.main()'`, icon: Play, iconColor: "var(--success)", shortcut: shortcutLabel("RunClass"), run: () => void launch(mainConfiguration(m)) },
            { label: `Debug '${m.name}.main()'`, icon: Bug, iconColor: "var(--success)", shortcut: shortcutLabel("DebugClass"), run: () => void debugConfiguration(mainConfiguration(m)) },
            { separator: true },
            { label: "Modify Run Configuration…", icon: Settings2, run: () => openRunConfigurations(mainConfiguration(m).name) },
          ],
        }),
    }));
  },
});

// Ctrl+Shift+F10 in a class with a main method runs it (tests go through the test gutter).
const runClass = commands().get("RunClass");
if (runClass) {
  registerCommand({
    ...runClass,
    run: async () => {
      const tab = activeTab();
      if (tab && tab.path.endsWith(".java") && active()) {
        const [main] = await javaApi.mainMethods(tab.path, tab.content).catch(() => [] as MainMethod[]);
        if (main) return void launch(mainConfiguration(main));
      }
      return runClass.run();
    },
  });
}
