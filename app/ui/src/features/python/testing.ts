/**
 * PyForge's test runners in the shared test runner: the pytest, Unittests and tox run
 * configuration types (command lines, reports and Rerun Failed Tests are built in Rust,
 * `packs/pyforge/testing`), the test gutter on Python test modules, Run Context
 * Configuration for plain Python files and "Run tox Environment…".
 */
import { Box, FlaskConical, TestTubeDiagonal } from "lucide-solid";
import type { RunConfiguration } from "@/ipc/run";
import { pythonToolsApi, type ToxEnv } from "@/ipc/pythonTools";
import { registerContextConfiguration, registerTestFileMatcher } from "@/features/run";
import { launch } from "@/features/run/runStore";
import { useTemporary } from "@/features/run/configStore";
import { baseName, registerRunConfigurationType, type RunConfigField } from "@/features/run/types";
import { item, showContextMenu, type MenuEntry } from "@/features/contextMenu/state";
import { showBalloon } from "@/keymap";
import { getSetting } from "@/state/settings";
import { currentInterpreter } from "./state";

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";
const scope = (cfg: RunConfiguration) => opt(cfg, "scope") || "all";
const inScope = (...scopes: string[]) => (c: RunConfiguration) => scopes.includes(scope(c));
const stem = (path: string) => baseName(path)?.replace(/\.py$/, "");
const defaultInterpreter = () => currentInterpreter()?.interpreter.path || getSetting<string>("python.interpreter", "") || "python3";

/** Python test modules: `test_*.py`, `*_test.py` and modules in `tests` / `test` folders. */
export const isPythonTestFile = (path: string) => /(^|\/)(test_[^/]*|[^/]*_test)\.py$/.test(path) || /(^|\/)tests?\/.+\.py$/.test(path);

registerTestFileMatcher(isPythonTestFile);

/** Run Context Configuration in a Python file without tests: the file as a script. */
registerContextConfiguration((path) =>
  /\.pyw?$/i.test(path) ? { name: stem(path) ?? path, type: "python", options: { target: "script", script: path } } : null,
);

const pytestFields: RunConfigField[] = [
  {
    key: "scope",
    label: "Target",
    kind: "segmented",
    default: "file",
    options: [
      { value: "all", label: "All tests" },
      { value: "directory", label: "Directory" },
      { value: "file", label: "Script path" },
      { value: "test", label: "Test" },
      { value: "keyword", label: "Keywords" },
    ],
  },
  { key: "directory", label: "Directory", kind: "path", directory: true, required: true, placeholder: "tests", visible: inScope("directory") },
  { key: "file", label: "Script path", kind: "path", required: true, placeholder: "tests/test_orders.py", visible: inScope("file", "test") },
  { key: "test", label: "Test", required: true, placeholder: "TestOrders::test_total[half-up]", visible: inScope("test") },
  { key: "keyword", label: "Keywords (-k)", required: true, placeholder: "orders and not slow", visible: inScope("keyword") },
];

registerRunConfigurationType({
  id: "pytest",
  name: "pytest",
  description: "Run pytest: all tests, a directory, a file, a class or test (node id), or tests matching keywords.",
  icon: FlaskConical,
  color: "var(--success)",
  pluginId: "forge.python-tools",
  interpreter: "custom",
  defaultInterpreter,
  argumentsLabel: "Additional arguments",
  debuggable: true,
  testRunner: true,
  coverage: true,
  defaults: () => ({ options: { scope: "file" } }),
  fields: pytestFields,
  suggestName: (cfg) => {
    switch (scope(cfg)) {
      case "directory":
        return `pytest in ${baseName(opt(cfg, "directory")) ?? ""}`;
      case "file":
        return opt(cfg, "file") ? `pytest in ${baseName(opt(cfg, "file"))}` : undefined;
      case "test":
        return opt(cfg, "test") ? `pytest for ${stem(opt(cfg, "file")) ?? ""}.${opt(cfg, "test").replace(/::/g, ".")}` : undefined;
      case "keyword":
        return opt(cfg, "keyword") ? `pytest -k ${opt(cfg, "keyword")}` : undefined;
      default:
        return "pytest";
    }
  },
});

registerRunConfigurationType({
  id: "unittest",
  name: "Unittests",
  description: "Run unittest test cases: discovery below a directory, a file, a class or a test method.",
  icon: TestTubeDiagonal,
  color: "var(--info)",
  pluginId: "forge.python-tools",
  interpreter: "custom",
  defaultInterpreter,
  argumentsLabel: false,
  debuggable: true,
  testRunner: true,
  defaults: () => ({ options: { scope: "file" } }),
  fields: [
    {
      key: "scope",
      label: "Target",
      kind: "segmented",
      default: "file",
      options: [
        { value: "all", label: "All tests" },
        { value: "directory", label: "Directory" },
        { value: "file", label: "Script path" },
        { value: "test", label: "Test" },
      ],
    },
    { key: "directory", label: "Directory", kind: "path", directory: true, required: true, placeholder: "tests", visible: inScope("directory") },
    { key: "pattern", label: "Pattern", placeholder: "test*.py", visible: inScope("all", "directory") },
    { key: "file", label: "Script path", kind: "path", required: true, placeholder: "tests/test_orders.py", visible: inScope("file", "test") },
    { key: "test", label: "Test", required: true, placeholder: "OrdersTest.test_total", visible: inScope("test") },
  ],
  suggestName: (cfg) => {
    switch (scope(cfg)) {
      case "directory":
        return `Unittests in ${baseName(opt(cfg, "directory")) ?? ""}`;
      case "file":
        return opt(cfg, "file") ? `Unittests in ${baseName(opt(cfg, "file"))}` : undefined;
      case "test":
        return opt(cfg, "test") ? `Unittests for ${stem(opt(cfg, "file")) ?? ""}.${opt(cfg, "test")}` : undefined;
      default:
        return "Unittests";
    }
  },
});

// ---- tox ---------------------------------------------------------------------------------

let toxCache: ToxEnv[] = [];

/** The project's tox environments (read from tox.ini, setup.cfg, pyproject.toml or tox.toml). */
export async function toxEnvironments(): Promise<ToxEnv[]> {
  toxCache = await pythonToolsApi.toxEnvironments().catch(() => toxCache);
  return toxCache;
}

registerRunConfigurationType({
  id: "tox",
  name: "tox",
  description: "Run tox environments; each environment shows as a test with its output.",
  icon: Box,
  color: "var(--syn-type)",
  pluginId: "forge.python-tools",
  interpreter: "custom",
  defaultInterpreter,
  argumentsLabel: "tox arguments",
  testRunner: true,
  fields: [
    {
      key: "environments",
      label: "Environments",
      placeholder: "py312, lint (empty: tox's env_list)",
    },
  ],
  suggestName: (cfg) => (opt(cfg, "environments") ? `tox: ${opt(cfg, "environments")}` : "tox"),
});

/** A temporary tox configuration for some environments ("" = the default env_list). */
export function toxConfiguration(environments: string): RunConfiguration {
  return { name: environments ? `tox: ${environments}` : "tox", type: "tox", options: { environments } };
}

/** "Run tox Environment…": a popup with the project's environments. */
export async function showToxPopup(x?: number, y?: number): Promise<void> {
  const envs = await toxEnvironments();
  if (!envs.length) {
    showBalloon("No tox environments", "Add a tox.ini, tox.toml or a [tool.tox] table to pyproject.toml.");
    return;
  }
  const entries: MenuEntry[] = [
    item("Run All (env_list)", () => void launch(useTemporary(toxConfiguration("")))),
    ...envs.map((e) => item(`${e.name}${e.description ? `  —  ${e.description}` : ""}`, () => void launch(useTemporary(toxConfiguration(e.name))))),
  ];
  showContextMenu(entries, x ?? window.innerWidth / 2 - 160, y ?? window.innerHeight / 3);
}
