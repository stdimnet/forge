/**
 * Java run configuration types, like IntelliJ IDEA's: Application, Maven, Gradle, JUnit,
 * TestNG and Remote JVM Debug. Command lines are built in Rust (`commands/java.rs`,
 * `javaforge-build`, `javaforge-lang-java`): JUnit and TestNG run through the build tool
 * (`cleanTest test --tests`, Surefire `-Dtest=`) and read its JUnit XML reports.
 */
import { Boxes, Coffee, FlaskConical, Hammer, Plug, TestTubeDiagonal } from "lucide-solid";
import type { RunConfiguration } from "@/ipc/run";
import { baseName, registerRunConfigurationType, type RunConfigField } from "@/features/run/types";
import { jdkChoices } from "./store";
import { t } from "@/i18n";

export const PLUGIN_ID = "forge.java";
export const APPLICATION = "java-application";
export const MAVEN = "maven";
export const GRADLE = "gradle";
export const REMOTE = "jvm-remote-debug";
export const TEST_TYPES = ["junit", "testng"];

/** Types the Java debugger debugs. */
export const DEBUG_TYPES = new Set([APPLICATION, GRADLE, REMOTE, ...TEST_TYPES]);

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";
const simple = (cls: string) => cls.split(/[.$]/).pop() || undefined;

const jdkField: RunConfigField = {
  key: "jdk",
  label: "JDK",
  kind: "enum",
  options: () => [{ value: "", label: t("langs.java.projectJdk") }, ...jdkChoices()],
};

const moduleField: RunConfigField = { key: "module", label: t("common.module"), placeholder: "app (empty: the root project)" };

registerRunConfigurationType({
  id: APPLICATION,
  name: "Application",
  description: t("langs.java.runAJavaClassWith"),
  icon: Coffee,
  color: "var(--syn-type)",
  pluginId: PLUGIN_ID,
  argumentsLabel: "Program arguments",
  debuggable: true,
  fields: [
    jdkField,
    { key: "mainClass", label: t("langs.java.mainClass"), required: true, placeholder: "com.acme.inventory.App" },
    { key: "interpreterOptions", label: t("common.vmOptions"), placeholder: "-Xmx512m -Dspring.profiles.active=dev" },
    moduleField,
    { key: "classpath", label: t("langs.java.classPath"), placeholder: t("langs.java.emptyFromTheBuildEclipse") },
    { key: "javaModule", label: t("langs.java.javaModule"), placeholder: "com.acme.inventory (runs with -m)" },
  ],
  suggestName: (cfg) => simple(opt(cfg, "mainClass")),
});

registerRunConfigurationType({
  id: GRADLE,
  name: "Gradle",
  description: t("langs.java.runGradleTasksWithThe"),
  icon: Boxes,
  color: "var(--success)",
  pluginId: PLUGIN_ID,
  argumentsLabel: "Arguments",
  debuggable: true,
  noRunTarget: true,
  fields: [
    { key: "tasks", label: t("langs.java.tasks"), required: true, placeholder: "clean build" },
    moduleField,
    jdkField,
  ],
  suggestName: (cfg) => {
    const tasks = opt(cfg, "tasks").trim();
    const module = opt(cfg, "module").trim();
    return tasks ? `${module ? `${module} ` : ""}[${tasks}]` : undefined;
  },
});

registerRunConfigurationType({
  id: MAVEN,
  name: "Maven",
  description: t("langs.java.runMavenGoalsWithThe"),
  icon: Hammer,
  color: "var(--danger)",
  pluginId: PLUGIN_ID,
  argumentsLabel: "Arguments",
  noRunTarget: true,
  fields: [
    { key: "goals", label: t("langs.java.commandLine"), required: true, placeholder: "clean install -DskipTests" },
    moduleField,
    jdkField,
  ],
  suggestName: (cfg) => {
    const goals = opt(cfg, "goals").trim();
    return goals ? `${opt(cfg, "module") || "project"} [${goals}]` : undefined;
  },
});

registerRunConfigurationType({
  id: REMOTE,
  name: "Remote JVM Debug",
  description: t("langs.java.attachTheDebuggerToA"),
  icon: Plug,
  color: "var(--info)",
  pluginId: PLUGIN_ID,
  argumentsLabel: false,
  debuggable: true,
  noRunTarget: true,
  fields: [
    { key: "host", label: t("langs.java.host"), default: "localhost", required: true },
    { key: "port", label: t("common.port"), default: "5005", required: true },
  ],
  validate: (cfg) => (/^\d+$/.test(opt(cfg, "port").trim()) ? null : "Port must be a number"),
  suggestName: (cfg) => `${opt(cfg, "host") || "localhost"}:${opt(cfg, "port") || "5005"}`,
});

const inScope =
  (...scopes: string[]) =>
  (cfg: RunConfiguration) =>
    scopes.includes(opt(cfg, "scope") || "class");

function testType(id: string, name: string) {
  registerRunConfigurationType({
    id,
    name,
    description: t("langs.java.runTestsThroughGradleOr", { name }),
    icon: id === "junit" ? FlaskConical : TestTubeDiagonal,
    color: "var(--success)",
    pluginId: PLUGIN_ID,
    argumentsLabel: false,
    debuggable: true,
    testRunner: true,
    coverage: true,
    noRunTarget: true,
    fields: [
      {
        key: "scope",
        label: t("common.testKind"),
        kind: "segmented",
        default: "class",
        options: [
          { value: "all", label: t("common.allInProject") },
          { value: "package", label: t("common.package") },
          { value: "class", label: t("common.class") },
          { value: "method", label: t("common.method") },
          { value: "pattern", label: t("langs.java.pattern") },
        ],
      },
      { key: "package", label: t("common.package"), required: true, placeholder: "com.acme.inventory", visible: inScope("package") },
      { key: "class", label: t("common.class"), required: true, placeholder: "com.acme.inventory.InventoryTest", visible: inScope("class", "method") },
      { key: "method", label: t("common.method"), required: true, placeholder: "addsStock", visible: inScope("method") },
      { key: "tests", label: t("langs.java.tests"), kind: "text", required: true, placeholder: "com.acme.InventoryTest#addsStock (one per line)", visible: inScope("pattern") },
      moduleField,
      { key: "testRunnerOptions", label: t("common.buildToolArguments"), placeholder: "--info" },
    ],
    suggestName: (cfg) => {
      switch (opt(cfg, "scope") || "class") {
        case "all":
          return `All in ${baseName(opt(cfg, "module")) ?? "project"}`;
        case "package":
          return opt(cfg, "package") ? `Tests in '${opt(cfg, "package")}'` : undefined;
        case "method":
          return opt(cfg, "method") ? `${simple(opt(cfg, "class")) ?? ""}.${opt(cfg, "method")}` : undefined;
        case "pattern":
          return `${name}: failed tests`;
        default:
          return simple(opt(cfg, "class"));
      }
    },
  });
}

testType("junit", "JUnit");
testType("testng", "TestNG");
