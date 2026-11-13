/**
 * Kotlin run configuration types, like IntelliJ IDEA's: Kotlin Script and Kotest. Kotlin
 * applications use JavaForge's Application type with `kotlin = true` (they run through the
 * build or kotlinc, see `commands/kotlin.rs`), and JUnit tests in Kotlin files its JUnit
 * type with `language = kotlin`.
 */
import { FileCode2, TestTubes } from "lucide-solid";
import type { RunConfiguration } from "@/ipc/run";
import { baseName, registerRunConfigurationType } from "@/features/run/types";
import { PLUGIN_ID } from "./store";
import { t } from "@/i18n";

export const SCRIPT = "kotlin-script";
export const KOTEST = "kotest";

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";
const simple = (cls: string) => cls.split(/[.$]/).pop() || undefined;

/** A configuration that runs Kotlin code (debugged with the Kotlin debugger). */
export const isKotlinConfiguration = (cfg: RunConfiguration) =>
  cfg.type === KOTEST ||
  (cfg.type === "java-application" && opt(cfg, "kotlin") === "true") ||
  ((cfg.type === "junit" || cfg.type === "testng") && opt(cfg, "language") === "kotlin");

registerRunConfigurationType({
  id: SCRIPT,
  name: "Kotlin Script",
  description: t("langs.kotlin.runAKotlinScriptKts"),
  icon: FileCode2,
  color: "var(--accent)",
  pluginId: PLUGIN_ID,
  argumentsLabel: "Script arguments",
  noRunTarget: true,
  fields: [
    { key: "script", label: t("langs.kotlin.scriptFile"), required: true, placeholder: "scripts/release.main.kts" },
    { key: "interpreterOptions", label: t("langs.kotlin.jvmOptions"), placeholder: "-Xmx1g" },
    { key: "kotlin", label: t("langs.kotlin.kotlinRunner"), placeholder: t("langs.kotlin.emptyKotlinOrKotlincScript") },
  ],
  validate: (cfg) => (/\.gradle\.kts$/.test(opt(cfg, "script")) ? "Gradle build scripts are not scripts to run" : null),
  suggestName: (cfg) => baseName(opt(cfg, "script")),
});

const inScope =
  (...scopes: string[]) =>
  (cfg: RunConfiguration) =>
    scopes.includes(opt(cfg, "scope") || "class");

registerRunConfigurationType({
  id: KOTEST,
  name: "Kotest",
  description: t("langs.kotlin.runKotestSpecsThroughGradle"),
  icon: TestTubes,
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
        { value: "class", label: t("langs.kotlin.spec") },
        { value: "test", label: t("common.test") },
        { value: "pattern", label: t("langs.kotlin.specs") },
      ],
    },
    { key: "package", label: t("common.package"), required: true, placeholder: "com.acme.orders", visible: inScope("package") },
    { key: "class", label: t("langs.kotlin.spec"), required: true, placeholder: "com.acme.orders.CartSpec", visible: inScope("class", "test") },
    { key: "test", label: t("common.test"), required: true, placeholder: "totals -- applies the discount", visible: inScope("test") },
    { key: "tests", label: t("langs.kotlin.specs"), kind: "text", required: true, placeholder: "com.acme.orders.CartSpec (one per line)", visible: inScope("pattern") },
    { key: "module", label: t("common.module"), placeholder: "app (empty: the root project)" },
    { key: "testRunnerOptions", label: t("common.buildToolArguments"), placeholder: "--info" },
  ],
  suggestName: (cfg) => {
    switch (opt(cfg, "scope") || "class") {
      case "all":
        return `All in ${baseName(opt(cfg, "module")) ?? "project"}`;
      case "package":
        return opt(cfg, "package") ? `Specs in '${opt(cfg, "package")}'` : undefined;
      case "test":
        return opt(cfg, "test") ? `${simple(opt(cfg, "class")) ?? ""}: ${opt(cfg, "test").split(" -- ").pop()}` : undefined;
      case "pattern":
        return "Kotest: failed specs";
      default:
        return simple(opt(cfg, "class"));
    }
  },
});
