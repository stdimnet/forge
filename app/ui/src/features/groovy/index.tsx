/**
 * Groovy in the UI (JavaForge), like IntelliJ IDEA's Groovy plugin. Highlighting comes from
 * the editor's Groovy mode; everything else is computed in Rust (`commands/groovy.rs`,
 * `packs/javaforge/lang-groovy`). This module plugs Groovy into the contribution points:
 *
 * - Settings | Languages & Frameworks | Groovy (the Groovy SDK and how scripts run);
 * - the Groovy Script and Spock run configuration types;
 * - the Run gutter on scripts and `main` methods, Run Context Configuration
 *   (Ctrl+Shift+F10). Spock specifications get the test gutter from the shared test runner.
 */
import { createResource, Show } from "solid-js";
import { CircleAlert, CircleCheck, FileCode2, Play, Settings2, TestTubes } from "lucide-solid";
import { groovyApi, type GroovyRunnable } from "@/ipc/groovy";
import type { RunConfiguration } from "@/ipc/run";
import { registerLineMarkerProvider, type LineMarker } from "@/editor/lineMarkers";
import { shortcutLabel } from "@/keymap";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { useTemporary } from "@/features/run/configStore";
import { launch } from "@/features/run/runStore";
import { openMenu } from "@/features/run/PopupMenu";
import { openRunConfigurations } from "@/features/run/dialogState";
import { baseName, registerRunConfigurationType } from "@/features/run/types";
import { commands, registerCommand } from "@/state/commands";
import { activeTab } from "@/state/editor";
import { isPluginActive } from "@/state/project";
import "@/features/java/java.css";
import "@/features/scala/scala.css";
import { t } from "@/i18n";

export const PLUGIN_ID = "forge.groovy";
export const SCRIPT = "groovy-script";
export const SPOCK = "spock";
const active = () => isPluginActive(PLUGIN_ID);

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";
const simple = (cls: string) => cls.split(/[.$]/).pop() || undefined;

/** Groovy sources (not Gradle build scripts). */
export const isGroovySource = (path: string) => /\.(groovy|gvy|gy)$/.test(path);

// ---- Settings -------------------------------------------------------------------------------

const SOURCES: Record<string, string> = {
  setting: "from the settings",
  GROOVY_HOME: "GROOVY_HOME",
  "SDKMAN!": "SDKMAN!",
  PATH: "on the PATH",
};

function GroovyPage() {
  const [status] = createResource(() => groovyApi.status().catch(() => null));
  return (
    <div class="java-page" data-groovy-page>
      <div class="java-status">
        <div class="java-status-row">
          <Show when={status()?.groovy} fallback={<CircleAlert size={14} class="java-warn" />}>
            <CircleCheck size={14} class="java-ok" />
          </Show>
          <span>
            {t("langs.groovy.groovySdkLabel")}
            <b>{status()?.groovy ? `Groovy ${status()?.version ?? ""} (${SOURCES[status()?.source ?? ""] ?? status()?.source})`.replace("  ", " ") : t("common.notFound")}</b>
            <Show when={status()?.groovy}>
              <span class="scala-path"> {status()!.groovy}</span>
            </Show>
          </span>
        </div>
        <div class="java-status-row">
          <CircleCheck size={14} class="java-ok" />
          <span>
            {t("langs.groovy.scriptsRunWith")} <b>{status()?.runner === "gradle" ? t("langs.groovy.gradleTheProjectSClass") : "groovy"}</b>
            <Show when={status()?.gradleGroovy}>
              <span class="muted"> {t("langs.groovy.theGradleBuildAppliesThe")}</span>
            </Show>
          </span>
        </div>
        <Show when={status() && !status()!.groovy && !status()!.gradleGroovy}>
          <p class="muted java-hint">{t("langs.groovy.installAGroovySdkWith")}</p>
        </Show>
      </div>
      <SchemaPage pageId="lang.groovy" />
    </div>
  );
}

registerSettingsPage({
  id: "lang.groovy",
  title: t("settings.page.lang.groovy"),
  group: "Languages & Frameworks",
  order: 27,
  keywords: ["groovy", "spock", "sdkman", "groovy sdk", "scripts"],
  component: GroovyPage,
});

// ---- Run configuration types -------------------------------------------------------------

registerRunConfigurationType({
  id: SCRIPT,
  name: "Groovy Script",
  description: "Run a Groovy script (or a class with a main method) with groovy from the Groovy SDK, or through Gradle with the project's class path.",
  icon: FileCode2,
  color: "var(--syn-type)",
  pluginId: PLUGIN_ID,
  argumentsLabel: "Script parameters",
  noRunTarget: true,
  fields: [
    { key: "script", label: "Script path", required: true, placeholder: "scripts/release.groovy" },
    { key: "interpreterOptions", label: "VM options", placeholder: "-Xmx512m" },
    {
      key: "runner",
      label: "Run with",
      kind: "enum",
      options: () => [
        { value: "", label: t("langs.groovy.settingsGroovyDefault") },
        { value: "groovy", label: "groovy (Groovy SDK)" },
        { value: "gradle", label: t("langs.groovy.gradleProjectClassPath") },
      ],
    },
    { key: "groovy", label: "Groovy launcher", placeholder: "Empty: the Groovy SDK from Settings | Groovy" },
  ],
  validate: (cfg) => (/\.gradle(\.kts)?$/.test(opt(cfg, "script")) ? "Gradle build scripts run through Gradle tasks" : null),
  suggestName: (cfg) => baseName(opt(cfg, "script")),
});

const inScope =
  (...scopes: string[]) =>
  (cfg: RunConfiguration) =>
    scopes.includes(opt(cfg, "scope") || "class");

registerRunConfigurationType({
  id: SPOCK,
  name: "Spock",
  description: "Run Spock specifications through Gradle or Maven: all, a package, a specification or one feature.",
  icon: TestTubes,
  color: "var(--success)",
  pluginId: PLUGIN_ID,
  argumentsLabel: false,
  testRunner: true,
  coverage: true,
  noRunTarget: true,
  defaults: () => ({ options: { language: "groovy" } }),
  fields: [
    {
      key: "scope",
      label: "Test kind",
      kind: "segmented",
      default: "class",
      options: [
        { value: "all", label: "All in project" },
        { value: "package", label: "Package" },
        { value: "class", label: "Specification" },
        { value: "method", label: "Feature" },
        { value: "pattern", label: "Pattern" },
      ],
    },
    { key: "package", label: "Package", required: true, placeholder: "com.acme.notes", visible: inScope("package") },
    { key: "class", label: "Specification", required: true, placeholder: "com.acme.notes.NoteSpec", visible: inScope("class", "method") },
    { key: "method", label: "Feature", required: true, placeholder: "saves a note", visible: inScope("method") },
    { key: "tests", label: "Tests", kind: "text", required: true, placeholder: "com.acme.notes.NoteSpec#saves a note (one per line)", visible: inScope("pattern") },
    { key: "module", label: "Module", placeholder: "app (empty: the root project)" },
    { key: "testRunnerOptions", label: "Build tool arguments", placeholder: "--info" },
  ],
  suggestName: (cfg) => {
    switch (opt(cfg, "scope") || "class") {
      case "all":
        return "All specifications";
      case "package":
        return opt(cfg, "package") ? `Specifications in '${opt(cfg, "package")}'` : undefined;
      case "method":
        return opt(cfg, "method") ? `${simple(opt(cfg, "class")) ?? ""}.${opt(cfg, "method")}` : undefined;
      case "pattern":
        return "Spock: failed features";
      default:
        return simple(opt(cfg, "class"));
    }
  },
});

// ---- Run gutter on scripts and main methods ---------------------------------------------------

function scriptConfiguration(path: string) {
  return useTemporary({ name: baseName(path) ?? path, type: SCRIPT, options: { script: path } });
}

function runMenu(label: string, cfg: () => ReturnType<typeof useTemporary>): LineMarker["onClick"] {
  return (_view, e) =>
    openMenu({
      x: e.clientX,
      y: e.clientY + 6,
      items: [
        { label: t("common.run3", { label }), icon: Play, iconColor: "var(--success)", shortcut: shortcutLabel("RunClass"), run: () => void launch(cfg()) },
        { separator: true },
        { label: t("common.modifyRunConfigurationEllipsis"), icon: Settings2, run: () => openRunConfigurations(cfg().name) },
      ],
    });
}

const runnableOf = (text: string) => groovyApi.runnable(text).catch((): GroovyRunnable => ({ script: false, mains: [] }));

registerLineMarkerProvider({
  appliesTo: isGroovySource,
  watch: active,
  markers: async (path, text) => {
    if (!active()) return [];
    const r = await runnableOf(text);
    const name = baseName(path) ?? path;
    if (r.script) return [{ line: 0, icon: Play, title: t("common.run", { name }), className: "marker-run", onClick: runMenu(name, () => scriptConfiguration(path)) }];
    return r.mains.map((m) => ({ line: m.line, icon: Play, title: t("common.run", { name: m.name }), className: "marker-run", onClick: runMenu(m.name, () => scriptConfiguration(path)) }));
  },
});

// Ctrl+Shift+F10 in a Groovy script (or a class with main) runs it.
const runClass = commands().get("RunClass");
if (runClass) {
  registerCommand({
    ...runClass,
    run: async () => {
      const tab = activeTab();
      if (tab && active() && isGroovySource(tab.path)) {
        const r = await runnableOf(tab.content);
        if (r.script || r.mains.length) return void launch(scriptConfiguration(tab.path));
      }
      return runClass.run();
    },
  });
}
