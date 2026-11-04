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

const moduleField: RunConfigField = { key: "module", label: "Module", placeholder: "app (empty: the root project)" };

registerRunConfigurationType({
  id: APPLICATION,
  name: "Application",
  description: "Run a Java class with a main method on the project JDK, with the class path of its module.",
  icon: Coffee,
  color: "var(--syn-type)",
  pluginId: PLUGIN_ID,
  argumentsLabel: "Program arguments",
  debuggable: true,
  fields: [
    jdkField,
    { key: "mainClass", label: "Main class", required: true, placeholder: "com.acme.inventory.App" },
    { key: "interpreterOptions", label: "VM options", placeholder: "-Xmx512m -Dspring.profiles.active=dev" },
    moduleField,
    { key: "classpath", label: "Class path", placeholder: "Empty: from the build (Eclipse JDT LS)" },
    { key: "javaModule", label: "Java module", placeholder: "com.acme.inventory (runs with -m)" },
  ],
  suggestName: (cfg) => simple(opt(cfg, "mainClass")),
});

registerRunConfigurationType({
  id: GRADLE,
  name: "Gradle",
  description: "Run Gradle tasks with the project's wrapper (gradlew) or the configured Gradle.",
  icon: Boxes,
  color: "var(--success)",
  pluginId: PLUGIN_ID,
  argumentsLabel: "Arguments",
  debuggable: true,
  noRunTarget: true,
  fields: [
    { key: "tasks", label: "Tasks", required: true, placeholder: "clean build" },
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
  description: "Run Maven goals with the project's wrapper (mvnw) or the configured Maven.",
  icon: Hammer,
  color: "var(--danger)",
  pluginId: PLUGIN_ID,
  argumentsLabel: "Arguments",
  noRunTarget: true,
  fields: [
    { key: "goals", label: "Command line", required: true, placeholder: "clean install -DskipTests" },
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
  description: "Attach the debugger to a JVM started with a JDWP agent (-agentlib:jdwp=…).",
  icon: Plug,
  color: "var(--info)",
  pluginId: PLUGIN_ID,
  argumentsLabel: false,
  debuggable: true,
  noRunTarget: true,
  fields: [
    { key: "host", label: "Host", default: "localhost", required: true },
    { key: "port", label: "Port", default: "5005", required: true },
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
    description: `Run ${name} tests through Gradle or Maven: all tests, a package, a class or a method.`,
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
        label: "Test kind",
        kind: "segmented",
        default: "class",
        options: [
          { value: "all", label: "All in project" },
          { value: "package", label: "Package" },
          { value: "class", label: "Class" },
          { value: "method", label: "Method" },
          { value: "pattern", label: "Pattern" },
        ],
      },
      { key: "package", label: "Package", required: true, placeholder: "com.acme.inventory", visible: inScope("package") },
      { key: "class", label: "Class", required: true, placeholder: "com.acme.inventory.InventoryTest", visible: inScope("class", "method") },
      { key: "method", label: "Method", required: true, placeholder: "addsStock", visible: inScope("method") },
      { key: "tests", label: "Tests", kind: "text", required: true, placeholder: "com.acme.InventoryTest#addsStock (one per line)", visible: inScope("pattern") },
      moduleField,
      { key: "testRunnerOptions", label: "Build tool arguments", placeholder: "--info" },
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
