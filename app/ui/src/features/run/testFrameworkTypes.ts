/**
 * Run configuration types of the PHP test frameworks besides PHPUnit: Pest, Codeception,
 * Behat and PHPSpec. Their command lines are built in Rust (`phpforge-test-frameworks`)
 * with the executables and default files of Settings | PHP | Test Frameworks; results show
 * in the test tree (Pest: TeamCity messages, the others: JUnit reports). All of them run with
 * coverage (Behat and PHPSpec through their code coverage extensions).
 */
import { FlaskConical, TestTubeDiagonal, Leaf, Microscope } from "lucide-solid";
import type { RunConfiguration } from "@/ipc/run";
import { baseName, registerRunConfigurationType, type RunConfigField } from "./types";
import { phpCoverageFields } from "./phpCoverage";

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";
const scope = (cfg: RunConfiguration) => opt(cfg, "scope") || "config";
const inScope = (...scopes: string[]) => (c: RunConfiguration) => scopes.includes(scope(c));
const stem = (path: string) => baseName(path)?.replace(/\.(php|feature)$/, "");

const CONFIG_SCOPE = { value: "config", label: "Defined in the configuration file" };

const common = (configPlaceholder: string): RunConfigField[] => [
  { key: "configFile", label: "Configuration file", kind: "path", placeholder: configPlaceholder },
  { key: "testRunnerOptions", label: "Test runner options", placeholder: "" },
];

registerRunConfigurationType({
  id: "pest",
  name: "Pest",
  description: "Run Pest tests: a directory, a file, a test or describe block, or phpunit.xml.",
  icon: Leaf,
  color: "var(--success)",
  pluginId: "forge.phpunit",
  interpreter: "php",
  argumentsLabel: false,
  debuggable: true,
  testRunner: true,
  coverage: true,
  coverageFields: phpCoverageFields("pest"),
  fields: [
    {
      key: "scope",
      label: "Test scope",
      kind: "segmented",
      default: "config",
      options: [
        { value: "directory", label: "Directory" },
        { value: "file", label: "File" },
        { value: "test", label: "Test" },
        CONFIG_SCOPE,
      ],
    },
    { key: "directory", label: "Directory", kind: "path", directory: true, required: true, placeholder: "tests/Feature", visible: inScope("directory") },
    { key: "file", label: "File", kind: "path", required: true, placeholder: "tests/Feature/CartTest.php", visible: inScope("file", "test") },
    { key: "test", label: "Test", required: true, placeholder: "it adds the tax", visible: inScope("test") },
    ...common("phpunit.xml (detected)"),
  ],
  suggestName: (cfg) => {
    switch (scope(cfg)) {
      case "directory":
        return baseName(opt(cfg, "directory"));
      case "file":
        return stem(opt(cfg, "file"));
      case "test":
        return opt(cfg, "test") || undefined;
      default:
        return "Pest";
    }
  },
});

registerRunConfigurationType({
  id: "codeception",
  name: "Codeception",
  description: "Run a Codeception suite, directory, file or test, with groups and environments.",
  icon: TestTubeDiagonal,
  color: "var(--syn-function)",
  pluginId: "forge.codeception",
  interpreter: "php",
  argumentsLabel: false,
  debuggable: true,
  testRunner: true,
  coverage: true,
  coverageFields: phpCoverageFields("codeception"),
  fields: [
    {
      key: "scope",
      label: "Test scope",
      kind: "segmented",
      default: "config",
      options: [
        { value: "suite", label: "Type" },
        { value: "directory", label: "Directory" },
        { value: "file", label: "File" },
        { value: "method", label: "Method" },
        CONFIG_SCOPE,
      ],
    },
    { key: "suite", label: "Type", required: true, placeholder: "Acceptance", visible: inScope("suite") },
    { key: "directory", label: "Directory", kind: "path", directory: true, required: true, placeholder: "tests/Acceptance", visible: inScope("directory") },
    { key: "file", label: "File", kind: "path", required: true, placeholder: "tests/Acceptance/LoginCest.php", visible: inScope("file", "method") },
    { key: "method", label: "Method", required: true, placeholder: "loginWithValidCredentials", visible: inScope("method") },
    { key: "group", label: "Groups", placeholder: "smoke, admin" },
    { key: "env", label: "Environments", placeholder: "chrome" },
    ...common("codeception.yml (detected)"),
  ],
  suggestName: (cfg) => {
    switch (scope(cfg)) {
      case "suite":
        return opt(cfg, "suite") || undefined;
      case "directory":
        return baseName(opt(cfg, "directory"));
      case "file":
        return stem(opt(cfg, "file"));
      case "method":
        return opt(cfg, "method") ? `${stem(opt(cfg, "file")) ?? ""}:${opt(cfg, "method")}` : undefined;
      default:
        return "Codeception";
    }
  },
});

registerRunConfigurationType({
  id: "behat",
  name: "Behat",
  description: "Run Behat features: a directory, a feature file or a scenario.",
  icon: Microscope,
  color: "var(--info)",
  pluginId: "forge.behat",
  interpreter: "php",
  argumentsLabel: false,
  debuggable: true,
  testRunner: true,
  coverage: true,
  coverageFields: phpCoverageFields("behat"),
  fields: [
    {
      key: "scope",
      label: "Test scope",
      kind: "segmented",
      default: "config",
      options: [
        { value: "directory", label: "Directory" },
        { value: "file", label: "File" },
        { value: "scenario", label: "Scenario" },
        CONFIG_SCOPE,
      ],
    },
    { key: "directory", label: "Directory", kind: "path", directory: true, required: true, placeholder: "features", visible: inScope("directory") },
    { key: "file", label: "Feature file", kind: "path", required: true, placeholder: "features/checkout.feature", visible: inScope("file", "scenario") },
    { key: "line", label: "Line", required: true, placeholder: "10", visible: inScope("scenario") },
    { key: "profile", label: "Profile", placeholder: "default" },
    ...common("behat.yml (detected)"),
  ],
  suggestName: (cfg) => {
    switch (scope(cfg)) {
      case "directory":
        return baseName(opt(cfg, "directory"));
      case "file":
        return stem(opt(cfg, "file"));
      case "scenario":
        return opt(cfg, "file") ? `${stem(opt(cfg, "file"))}:${opt(cfg, "line")}` : undefined;
      default:
        return "Behat";
    }
  },
});

registerRunConfigurationType({
  id: "phpspec",
  name: "PHPSpec",
  description: "Run PHPSpec specs: a directory, a spec file or an example.",
  icon: FlaskConical,
  color: "var(--warning)",
  pluginId: "forge.phpspec",
  interpreter: "php",
  argumentsLabel: false,
  debuggable: true,
  testRunner: true,
  coverage: true,
  coverageFields: phpCoverageFields("phpspec"),
  fields: [
    {
      key: "scope",
      label: "Test scope",
      kind: "segmented",
      default: "config",
      options: [
        { value: "directory", label: "Directory" },
        { value: "file", label: "Spec" },
        { value: "method", label: "Example" },
        CONFIG_SCOPE,
      ],
    },
    { key: "directory", label: "Directory", kind: "path", directory: true, required: true, placeholder: "spec", visible: inScope("directory") },
    { key: "file", label: "Spec file", kind: "path", required: true, placeholder: "spec/App/Services/CheckoutSpec.php", visible: inScope("file", "method") },
    { key: "line", label: "Line", required: true, placeholder: "14", visible: inScope("method") },
    { key: "method", label: "Example", placeholder: "it_adds_the_tax_to_the_total", visible: inScope("method") },
    ...common("phpspec.yml (detected)"),
  ],
  suggestName: (cfg) => {
    switch (scope(cfg)) {
      case "directory":
        return baseName(opt(cfg, "directory"));
      case "file":
        return stem(opt(cfg, "file"));
      case "method":
        return opt(cfg, "method") ? `${stem(opt(cfg, "file")) ?? ""}::${opt(cfg, "method")}` : undefined;
      default:
        return "PHPSpec";
    }
  },
});
