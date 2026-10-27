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
  description: "Run a Kotlin script (.kts, .main.kts) with kotlin or kotlinc -script. Gradle build scripts run through Gradle.",
  icon: FileCode2,
  color: "var(--accent)",
  pluginId: PLUGIN_ID,
  argumentsLabel: "Script arguments",
  noRunTarget: true,
  fields: [
    { key: "script", label: "Script file", required: true, placeholder: "scripts/release.main.kts" },
    { key: "interpreterOptions", label: "JVM options", placeholder: "-Xmx1g" },
    { key: "kotlin", label: "Kotlin runner", placeholder: "Empty: kotlin (or kotlinc -script) from Settings | Kotlin" },
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
  description: "Run Kotest specs through Gradle or Maven: all, a package, a spec or a single test.",
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
      label: "Test kind",
      kind: "segmented",
      default: "class",
      options: [
        { value: "all", label: "All in project" },
        { value: "package", label: "Package" },
        { value: "class", label: "Spec" },
        { value: "test", label: "Test" },
        { value: "pattern", label: "Specs" },
      ],
    },
    { key: "package", label: "Package", required: true, placeholder: "com.acme.orders", visible: inScope("package") },
    { key: "class", label: "Spec", required: true, placeholder: "com.acme.orders.CartSpec", visible: inScope("class", "test") },
    { key: "test", label: "Test", required: true, placeholder: "totals -- applies the discount", visible: inScope("test") },
    { key: "tests", label: "Specs", kind: "text", required: true, placeholder: "com.acme.orders.CartSpec (one per line)", visible: inScope("pattern") },
    { key: "module", label: "Module", placeholder: "app (empty: the root project)" },
    { key: "testRunnerOptions", label: "Build tool arguments", placeholder: "--info" },
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
