/**
 * The Spring Boot run configuration (IntelliJ IDEA's "Spring Boot"): main class, active
 * profiles, override properties, VM options, the JDK and the actuator endpoints the run
 * dashboard reads. The backend runs it as an Application configuration
 * (`commands/spring.rs`, `javaforge_fw_spring::run`), so Gradle's `forgeRun`, JDT LS's class
 * path, Kotlin applications and the Java debugger work as for Application runs. Also the ▶
 * gutter icon on `@SpringBootApplication` classes.
 */
import { Bug, Leaf, Play, Settings2 } from "lucide-solid";
import { SPRING_BOOT, type SpringApplication } from "@/ipc/spring";
import type { RunConfiguration } from "@/ipc/run";
import { registerRunConfigurationType } from "@/features/run/types";
import { registerLineMarkerProvider } from "@/editor/lineMarkers";
import { useTemporary } from "@/features/run/configStore";
import { debugConfiguration, launch } from "@/features/run/runStore";
import { openMenu } from "@/features/run/PopupMenu";
import { openRunConfigurations } from "@/features/run/dialogState";
import { shortcutLabel } from "@/keymap";
import { DEBUG_TYPES } from "@/features/java/runTypes";
import { jdkChoices } from "@/features/java/store";
import { overview, PLUGIN_ID, springActive } from "./store";
import { t } from "@/i18n";

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";
const simple = (cls: string) => cls.split(/[.$]/).pop() || undefined;
const springApps = () => (overview()?.applications ?? []).filter((a) => a.kind === "spring");

registerRunConfigurationType({
  id: SPRING_BOOT,
  name: "Spring Boot",
  description: t("langs.spring.runASpringBootApplication"),
  icon: Leaf,
  color: "var(--success)",
  pluginId: PLUGIN_ID,
  argumentsLabel: "Program arguments",
  debuggable: true,
  fields: [
    { key: "mainClass", label: t("langs.spring.mainClass"), required: true, placeholder: springApps()[0]?.mainClass ?? "com.acme.shop.ShopApplication" },
    { key: "activeProfiles", label: t("langs.spring.activeProfiles"), placeholder: "dev, local (comma-separated)" },
    { key: "overrideProperties", label: t("langs.spring.overrideProperties"), kind: "text", placeholder: "server.port=9090 (one key=value per line)" },
    { key: "interpreterOptions", label: t("common.vmOptions"), placeholder: "-Xmx1g" },
    {
      key: "jdk",
      label: "JDK",
      kind: "enum",
      options: () => [{ value: "", label: t("langs.spring.projectJdk") }, ...jdkChoices()],
    },
    { key: "module", label: t("common.module"), placeholder: "app (empty: the root project)" },
    {
      key: "exposeActuator",
      label: t("langs.spring.runDashboard"),
      kind: "segmented",
      default: "true",
      options: [
        { value: "true", label: t("langs.spring.exposeActuatorHealthBeansMappings") },
        { value: "false", label: t("langs.spring.leaveTheActuatorAsConfigured") },
      ],
    },
  ],
  defaults: () => {
    const app = springApps()[0];
    return app ? { options: { mainClass: app.mainClass, ...(app.kotlin ? { kotlin: "true" } : {}), exposeActuator: "true" } } : {};
  },
  suggestName: (cfg) => simple(opt(cfg, "mainClass"))?.replace(/Kt$/, ""),
  validate: (cfg) => (/^[\w$.]+$/.test(opt(cfg, "mainClass").trim()) ? null : "Main class must be a fully qualified class name"),
});

// The Java debugger debugs Spring Boot runs (they are Application runs).
DEBUG_TYPES.add(SPRING_BOOT);

/** The temporary Spring Boot configuration of an application class. */
export function bootConfiguration(app: SpringApplication) {
  return useTemporary({
    name: app.name,
    type: SPRING_BOOT,
    options: { mainClass: app.mainClass, exposeActuator: "true", ...(app.kotlin ? { kotlin: "true" } : {}) },
  });
}

registerLineMarkerProvider({
  appliesTo: (path) => /\.(java|kt)$/.test(path),
  watch: () => [springActive(), overview()],
  markers: async (path) => {
    if (!springActive()) return [];
    return springApps()
      .filter((a) => a.location.path === path)
      .map((a) => ({
        line: a.location.line,
        icon: Play,
        title: t("langs.spring.runSpringBoot", { name: a.name }),
        className: "marker-run",
        onClick: (_view, e) =>
          openMenu({
            x: e.clientX,
            y: e.clientY + 6,
            items: [
              { label: t("common.run", { name: a.name }), icon: Play, iconColor: "var(--success)", shortcut: shortcutLabel("RunClass"), run: () => void launch(bootConfiguration(a)) },
              { label: t("common.debug", { name: a.name }), icon: Bug, iconColor: "var(--success)", shortcut: shortcutLabel("DebugClass"), run: () => void debugConfiguration(bootConfiguration(a)) },
              { separator: true },
              { label: t("common.modifyRunConfigurationEllipsis"), icon: Settings2, run: () => openRunConfigurations(bootConfiguration(a).name) },
            ],
          }),
      }));
  },
});
