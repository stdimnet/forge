/**
 * Built-in run configuration types: PHP Script, PHP Built-in Web Server, PHPUnit, Artisan,
 * Composer, Shell, npm and Compound.
 */
import { FileCode, FlaskConical, Globe, Hexagon, Layers, Package, SquareTerminal, Feather } from "lucide-solid";
import type { RunConfiguration } from "@/ipc/run";
import { CompoundForm } from "./CompoundForm";
import { baseName, COMPOUND, compoundMembers, registerRunConfigurationType } from "./types";
import { phpCoverageFields } from "./phpCoverage";
import { t } from "@/i18n";

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";
const shortClass = (fqcn: string) => fqcn.split("\\").pop() ?? fqcn;

registerRunConfigurationType({
  id: "php-script",
  name: "PHP Script",
  description: t("run.builtinTypes.runAPhpFileWith"),
  icon: FileCode,
  color: "var(--syn-variable)",
  pluginId: "forge.php",
  interpreter: "php",
  argumentsLabel: "Arguments",
  debuggable: true,
  fields: [{ key: "file", label: t("run.builtinTypes.file"), kind: "path", required: true, placeholder: "bin/console.php" }],
  suggestName: (cfg) => baseName(opt(cfg, "file")),
});

/** The URL a built-in web server configuration serves. */
export function builtinServerUrl(cfg: RunConfiguration): string {
  const host = opt(cfg, "host").trim() || "localhost";
  const shown = host === "0.0.0.0" || host === "[::]" ? "localhost" : host;
  return `http://${shown}:${opt(cfg, "port").trim() || "8080"}/`;
}

registerRunConfigurationType({
  id: "php-builtin-server",
  name: "PHP Built-in Web Server",
  description: t("run.builtinTypes.serveADocumentRootWith"),
  icon: Globe,
  color: "var(--info)",
  pluginId: "forge.php",
  interpreter: "php",
  argumentsLabel: false,
  debuggable: true,
  fields: [
    { key: "host", label: t("run.builtinTypes.host"), default: "localhost", placeholder: "localhost" },
    { key: "port", label: t("run.builtinTypes.port"), default: "8080", placeholder: "8080" },
    { key: "documentRoot", label: t("run.builtinTypes.documentRoot"), kind: "path", directory: true, placeholder: "public (empty: the project root)" },
    { key: "useRouter", label: t("run.builtinTypes.useRouterScript"), kind: "bool" },
    {
      key: "routerScript",
      label: t("run.builtinTypes.routerScript"),
      kind: "path",
      required: true,
      placeholder: "public/router.php",
      visible: (c) => opt(c, "useRouter") === "true",
    },
    { key: "openBrowser", label: t("run.builtinTypes.openInBrowserAfterLaunch"), kind: "bool", default: "true" },
  ],
  suggestName: (cfg) => `${opt(cfg, "host") || "localhost"}:${opt(cfg, "port") || "8080"}`,
  afterStart: (cfg, handle) => {
    if ((cfg.options?.openBrowser ?? "true") !== "true") return;
    let exited = false;
    void handle.exited.then(() => (exited = true));
    // Give `php -S` a moment to bind its port; a server that failed to start opens nothing.
    setTimeout(() => !exited && window.open(builtinServerUrl(cfg), "_blank", "noopener"), 800);
  },
});

const scope = (cfg: RunConfiguration) => opt(cfg, "scope") || "config";

registerRunConfigurationType({
  id: "phpunit",
  name: "PHPUnit",
  description: t("run.builtinTypes.runATestClassMethod"),
  icon: FlaskConical,
  color: "var(--success)",
  pluginId: "forge.phpunit",
  interpreter: "php",
  argumentsLabel: false,
  debuggable: true,
  testRunner: true,
  coverage: true,
  coverageFields: phpCoverageFields("phpunit"),
  fields: [
    {
      key: "scope",
      label: t("run.builtinTypes.testScope"),
      kind: "segmented",
      default: "config",
      options: [
        { value: "directory", label: t("run.builtinTypes.directory") },
        { value: "file", label: t("run.builtinTypes.file") },
        { value: "class", label: t("run.builtinTypes.class") },
        { value: "method", label: t("run.builtinTypes.method") },
        { value: "config", label: t("run.builtinTypes.definedInTheConfigurationFile") },
      ],
    },
    {
      key: "directory",
      label: t("run.builtinTypes.directory"),
      kind: "path",
      directory: true,
      required: true,
      placeholder: "tests/Unit",
      visible: (c) => scope(c) === "directory",
    },
    {
      key: "class",
      label: t("run.builtinTypes.class"),
      required: true,
      placeholder: "Tests\\Unit\\UserTest",
      visible: (c) => scope(c) === "class" || scope(c) === "method",
    },
    {
      key: "method",
      label: t("run.builtinTypes.method"),
      required: true,
      placeholder: "test_full_name",
      visible: (c) => scope(c) === "method",
    },
    {
      key: "file",
      label: t("run.builtinTypes.file"),
      kind: "path",
      required: true,
      placeholder: "tests/Unit/UserTest.php",
      visible: (c) => ["file", "class", "method"].includes(scope(c)),
    },
    { key: "configFile", label: t("run.builtinTypes.configurationFile"), kind: "path", placeholder: "phpunit.xml (detected)" },
    { key: "testRunnerOptions", label: t("run.builtinTypes.testRunnerOptions"), placeholder: "--stop-on-failure" },
  ],
  suggestName: (cfg) => {
    switch (scope(cfg)) {
      case "directory":
        return baseName(opt(cfg, "directory"));
      case "file":
        return baseName(opt(cfg, "file"))?.replace(/\.php$/, "");
      case "class":
        return opt(cfg, "class") ? shortClass(opt(cfg, "class")) : undefined;
      case "method":
        return opt(cfg, "method") ? `${shortClass(opt(cfg, "class"))}::${opt(cfg, "method")}` : undefined;
      default:
        return baseName(opt(cfg, "configFile")) ?? "tests";
    }
  },
});

registerRunConfigurationType({
  id: "artisan",
  name: "Artisan",
  description: t("run.builtinTypes.runAnArtisanCommandOf"),
  icon: Feather,
  color: "var(--danger)",
  pluginId: "forge.laravel",
  interpreter: "php",
  argumentsLabel: "Arguments",
  debuggable: true,
  fields: [{ key: "command", label: t("run.builtinTypes.command"), required: true, placeholder: "migrate --seed" }],
  suggestName: (cfg) => (opt(cfg, "command") ? `artisan ${opt(cfg, "command")}` : undefined),
});

registerRunConfigurationType({
  id: "composer",
  name: "Composer Script",
  description: t("run.builtinTypes.runAScriptFromComposer"),
  icon: Package,
  color: "var(--syn-function)",
  pluginId: "forge.composer",
  argumentsLabel: "Arguments",
  fields: [{ key: "script", label: t("run.builtinTypes.script"), required: true, placeholder: "test" }],
  suggestName: (cfg) => opt(cfg, "script") || undefined,
});

registerRunConfigurationType({
  id: "shell",
  name: "Shell Script",
  description: t("run.builtinTypes.runAScriptFileOr"),
  icon: SquareTerminal,
  color: "var(--fg-muted)",
  pluginId: "forge.run",
  interpreter: "shell",
  argumentsLabel: "Script options",
  fields: [
    {
      key: "mode",
      label: t("run.builtinTypes.execute"),
      kind: "segmented",
      default: "file",
      options: [
        { value: "file", label: t("run.builtinTypes.scriptFile") },
        { value: "text", label: t("run.builtinTypes.scriptText") },
      ],
    },
    {
      key: "scriptPath",
      label: t("run.builtinTypes.scriptPath"),
      kind: "path",
      required: true,
      placeholder: "scripts/deploy.sh",
      visible: (c) => opt(c, "mode") !== "text",
    },
    {
      key: "scriptText",
      label: t("run.builtinTypes.scriptText"),
      kind: "text",
      required: true,
      placeholder: "php artisan optimize:clear",
      visible: (c) => opt(c, "mode") === "text",
    },
  ],
  suggestName: (cfg) => (opt(cfg, "mode") === "text" ? undefined : baseName(opt(cfg, "scriptPath"))),
});

registerRunConfigurationType({
  id: "npm",
  name: "npm",
  description: t("run.builtinTypes.runAPackageJsonScript"),
  icon: Hexagon,
  color: "var(--danger)",
  pluginId: "forge.web",
  argumentsLabel: "Arguments",
  debuggable: true,
  fields: [
    { key: "packageJson", label: "package.json", kind: "path", default: "package.json" },
    {
      key: "command",
      label: t("run.builtinTypes.command"),
      kind: "enum",
      default: "run",
      options: ["run", "install", "ci", "test", "start"].map((v) => ({ value: v, label: v })),
    },
    {
      key: "script",
      label: t("run.builtinTypes.scripts"),
      required: true,
      placeholder: "dev",
      visible: (c) => (opt(c, "command") || "run") === "run",
    },
    {
      key: "packageManager",
      label: t("run.builtinTypes.packageManager"),
      kind: "enum",
      default: "npm",
      options: ["npm", "pnpm", "yarn", "bun"].map((v) => ({ value: v, label: v })),
    },
  ],
  suggestName: (cfg) => opt(cfg, "script") || opt(cfg, "command") || undefined,
});

registerRunConfigurationType({
  id: COMPOUND,
  name: "Compound",
  description: t("run.builtinTypes.startSeveralConfigurationsTogetherEach"),
  icon: Layers,
  color: "var(--accent)",
  argumentsLabel: false,
  debuggable: true,
  fields: [],
  form: CompoundForm,
  validate: (cfg) => (compoundMembers(cfg).length ? null : t("run.builtinTypes.noConfigurationsToRun")),
});
