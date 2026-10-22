/**
 * Built-in run configuration types: PHP Script, PHP Built-in Web Server, PHPUnit, Artisan,
 * Composer, Shell, npm and Compound.
 */
import { FileCode, FlaskConical, Globe, Hexagon, Layers, Package, SquareTerminal, Feather } from "lucide-solid";
import type { RunConfiguration } from "@/ipc/run";
import { CompoundForm } from "./CompoundForm";
import { baseName, COMPOUND, compoundMembers, registerRunConfigurationType } from "./types";

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";
const shortClass = (fqcn: string) => fqcn.split("\\").pop() ?? fqcn;

registerRunConfigurationType({
  id: "php-script",
  name: "PHP Script",
  description: "Run a PHP file with the CLI interpreter.",
  icon: FileCode,
  color: "var(--syn-variable)",
  pluginId: "forge.php",
  interpreter: "php",
  argumentsLabel: "Arguments",
  debuggable: true,
  fields: [{ key: "file", label: "File", kind: "path", required: true, placeholder: "bin/console.php" }],
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
  description: "Serve a document root with php -S; Debug debugs every request.",
  icon: Globe,
  color: "var(--info)",
  pluginId: "forge.php",
  interpreter: "php",
  argumentsLabel: false,
  debuggable: true,
  fields: [
    { key: "host", label: "Host", default: "localhost", placeholder: "localhost" },
    { key: "port", label: "Port", default: "8080", placeholder: "8080" },
    { key: "documentRoot", label: "Document root", kind: "path", directory: true, placeholder: "public (empty: the project root)" },
    { key: "useRouter", label: "Use router script", kind: "bool" },
    {
      key: "routerScript",
      label: "Router script",
      kind: "path",
      required: true,
      placeholder: "public/router.php",
      visible: (c) => opt(c, "useRouter") === "true",
    },
    { key: "openBrowser", label: "Open in browser after launch", kind: "bool", default: "true" },
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
  description: "Run a test class, method, directory or phpunit.xml configuration.",
  icon: FlaskConical,
  color: "var(--success)",
  pluginId: "forge.phpunit",
  interpreter: "php",
  argumentsLabel: false,
  debuggable: true,
  testRunner: true,
  fields: [
    {
      key: "scope",
      label: "Test scope",
      kind: "segmented",
      default: "config",
      options: [
        { value: "directory", label: "Directory" },
        { value: "file", label: "File" },
        { value: "class", label: "Class" },
        { value: "method", label: "Method" },
        { value: "config", label: "Defined in the configuration file" },
      ],
    },
    {
      key: "directory",
      label: "Directory",
      kind: "path",
      directory: true,
      required: true,
      placeholder: "tests/Unit",
      visible: (c) => scope(c) === "directory",
    },
    {
      key: "class",
      label: "Class",
      required: true,
      placeholder: "Tests\\Unit\\UserTest",
      visible: (c) => scope(c) === "class" || scope(c) === "method",
    },
    {
      key: "method",
      label: "Method",
      required: true,
      placeholder: "test_full_name",
      visible: (c) => scope(c) === "method",
    },
    {
      key: "file",
      label: "File",
      kind: "path",
      required: true,
      placeholder: "tests/Unit/UserTest.php",
      visible: (c) => ["file", "class", "method"].includes(scope(c)),
    },
    { key: "configFile", label: "Configuration file", kind: "path", placeholder: "phpunit.xml (detected)" },
    { key: "testRunnerOptions", label: "Test runner options", placeholder: "--stop-on-failure" },
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
  description: "Run an artisan command of a Laravel application.",
  icon: Feather,
  color: "var(--danger)",
  pluginId: "forge.laravel",
  interpreter: "php",
  argumentsLabel: "Arguments",
  debuggable: true,
  fields: [{ key: "command", label: "Command", required: true, placeholder: "migrate --seed" }],
  suggestName: (cfg) => (opt(cfg, "command") ? `artisan ${opt(cfg, "command")}` : undefined),
});

registerRunConfigurationType({
  id: "composer",
  name: "Composer Script",
  description: "Run a script from composer.json.",
  icon: Package,
  color: "var(--syn-function)",
  pluginId: "forge.composer",
  argumentsLabel: "Arguments",
  fields: [{ key: "script", label: "Script", required: true, placeholder: "test" }],
  suggestName: (cfg) => opt(cfg, "script") || undefined,
});

registerRunConfigurationType({
  id: "shell",
  name: "Shell Script",
  description: "Run a script file or a script text with a shell.",
  icon: SquareTerminal,
  color: "var(--fg-muted)",
  pluginId: "forge.run",
  interpreter: "shell",
  argumentsLabel: "Script options",
  fields: [
    {
      key: "mode",
      label: "Execute",
      kind: "segmented",
      default: "file",
      options: [
        { value: "file", label: "Script file" },
        { value: "text", label: "Script text" },
      ],
    },
    {
      key: "scriptPath",
      label: "Script path",
      kind: "path",
      required: true,
      placeholder: "scripts/deploy.sh",
      visible: (c) => opt(c, "mode") !== "text",
    },
    {
      key: "scriptText",
      label: "Script text",
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
  description: "Run a package.json script.",
  icon: Hexagon,
  color: "var(--danger)",
  pluginId: "forge.web",
  argumentsLabel: "Arguments",
  debuggable: true,
  fields: [
    { key: "packageJson", label: "package.json", kind: "path", default: "package.json" },
    {
      key: "command",
      label: "Command",
      kind: "enum",
      default: "run",
      options: ["run", "install", "ci", "test", "start"].map((v) => ({ value: v, label: v })),
    },
    {
      key: "script",
      label: "Scripts",
      required: true,
      placeholder: "dev",
      visible: (c) => (opt(c, "command") || "run") === "run",
    },
    {
      key: "packageManager",
      label: "Package manager",
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
  description: "Start several configurations together, each in its own Run tab.",
  icon: Layers,
  color: "var(--accent)",
  argumentsLabel: false,
  debuggable: true,
  fields: [],
  form: CompoundForm,
  validate: (cfg) => (compoundMembers(cfg).length ? null : "No configurations to run"),
});
