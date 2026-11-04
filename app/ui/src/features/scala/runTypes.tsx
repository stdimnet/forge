/**
 * Scala run configuration types, like IntelliJ IDEA's: sbt Task (tasks such as
 * `~compile`, `test`, `runMain com.acme.Main`, or the sbt shell without tasks), ScalaTest
 * and MUnit (through sbt `testOnly` with JUnit XML reports). Command lines are built in Rust
 * (`commands/scala.rs`, `javaforge-lang-scala`).
 */
import { FlaskConical, SquareTerminal, TestTubes } from "lucide-solid";
import type { RunConfiguration } from "@/ipc/run";
import { registerRunConfigurationType, type RunConfigField } from "@/features/run/types";
import { PLUGIN_ID } from "./store";
import { t } from "@/i18n";

export const SBT_TASK = "sbt-task";
export const SCALATEST = "scalatest";
export const MUNIT = "munit";

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";
const simple = (cls: string) => cls.split(/[.$]/).pop() || undefined;

registerRunConfigurationType({
  id: SBT_TASK,
  name: "sbt Task",
  description: t("langs.scala.runSbtTasksOnePer"),
  icon: SquareTerminal,
  color: "var(--accent)",
  pluginId: PLUGIN_ID,
  argumentsLabel: "sbt options",
  noRunTarget: true,
  fields: [
    { key: "tasks", label: t("langs.scala.tasks"), kind: "text", placeholder: "~compile (empty: the sbt shell)" },
    { key: "project", label: t("common.project"), placeholder: "core (empty: the root project)" },
    { key: "interpreterOptions", label: t("langs.scala.vmParameters"), placeholder: "-Xmx2g" },
    { key: "sbt", label: "sbt launcher", placeholder: t("langs.scala.emptySettingsScalaSdkmanOr") },
  ],
  suggestName: (cfg) => {
    const tasks = opt(cfg, "tasks").trim().split(/[\n;]/)[0]?.trim();
    const project = opt(cfg, "project").trim();
    if (!tasks) return "sbt shell";
    return project ? `${project}/${tasks}` : tasks;
  },
});

const inScope =
  (...scopes: string[]) =>
  (cfg: RunConfiguration) =>
    scopes.includes(opt(cfg, "scope") || "class");

function testFields(): RunConfigField[] {
  return [
    {
      key: "scope",
      label: t("langs.scala.testKind"),
      kind: "segmented",
      default: "class",
      options: [
        { value: "all", label: t("langs.scala.allInProject") },
        { value: "package", label: t("common.package") },
        { value: "class", label: t("langs.scala.suite") },
        { value: "test", label: t("common.test") },
        { value: "pattern", label: t("langs.scala.suites") },
      ],
    },
    { key: "package", label: t("common.package"), required: true, placeholder: "com.acme.shop", visible: inScope("package") },
    { key: "class", label: t("langs.scala.suite"), required: true, placeholder: "com.acme.shop.CartSpec", visible: inScope("class", "test") },
    { key: "test", label: t("langs.scala.testName"), required: true, placeholder: t("langs.scala.aCartShouldAddItems"), visible: inScope("test") },
    { key: "tests", label: t("langs.scala.suites"), kind: "text", required: true, placeholder: "com.acme.shop.CartSpec (one per line)", visible: inScope("pattern") },
    { key: "project", label: "sbt project", placeholder: "core (empty: the root project)" },
    { key: "sbtOptions", label: "sbt options", placeholder: "-J-Xmx2g" },
  ];
}

const testName = (label: string) => (cfg: RunConfiguration) => {
  switch (opt(cfg, "scope") || "class") {
    case "all":
      return `All ${label} suites`;
    case "package":
      return opt(cfg, "package") ? `Suites in '${opt(cfg, "package")}'` : undefined;
    case "test":
      return opt(cfg, "test") ? `${simple(opt(cfg, "class")) ?? ""}: ${opt(cfg, "test")}` : undefined;
    case "pattern":
      return `${label}: failed suites`;
    default:
      return simple(opt(cfg, "class"));
  }
};

registerRunConfigurationType({
  id: SCALATEST,
  name: "ScalaTest",
  description: t("langs.scala.runScalatestSuitesThroughSbt"),
  icon: TestTubes,
  color: "var(--success)",
  pluginId: PLUGIN_ID,
  argumentsLabel: false,
  testRunner: true,
  noRunTarget: true,
  fields: testFields(),
  suggestName: testName("ScalaTest"),
});

registerRunConfigurationType({
  id: MUNIT,
  name: "MUnit",
  description: t("langs.scala.runMunitSuitesThroughSbt"),
  icon: FlaskConical,
  color: "var(--success)",
  pluginId: PLUGIN_ID,
  argumentsLabel: false,
  testRunner: true,
  noRunTarget: true,
  fields: testFields(),
  suggestName: testName("MUnit"),
});
