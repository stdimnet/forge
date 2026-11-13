/**
 * The JavaScript debugger and the Node.js run configurations (forge.js-debugger), and the
 * actions of the Node.js test runners (forge.js-testing).
 *
 * - Run configuration types "Node.js", "JavaScript Debug" and "Attach to Node.js/Chrome".
 *   Node.js runs through the backend (the manifest's command template), Run with Coverage
 *   under c8 or nyc; JavaScript Debug
 *   opens its URL in the browser; Attach only debugs.
 * - Debug (Shift+F9) of these, of npm scripts and of the test runners (Jest, Vitest,
 *   Mocha, Playwright, node --test) starts js-debug: Rust builds the launch request
 *   (`js_debug_request`), the shared debugger runs the session (`dap_start`). The first
 *   time, Forge asks to download js-debug (pinned release, SHA-256 checked).
 * - Actions: Download JavaScript Debugger, Show Playwright Report, Open Cypress.
 */
import { Globe, Hexagon, Plug } from "lucide-solid";
import { registerDebugLauncher, registerRunConfigurationType, baseName, type DebugLaunchContext } from "@/features/run/types";
import { useTemporary } from "@/features/run/configStore";
import { runConfiguration } from "@/features/run/runStore";
import { refreshDebugAdapters } from "@/features/debug/adapters";
import { dapApi } from "@/features/debug/dapApi";
import { confirmDialog } from "@/features/browserCompanion/ConfirmDialog";
import { jsDebugApi, type JsDebugStatus } from "@/ipc/jsDebug";
import { runApi, type RunConfiguration } from "@/ipc/run";
import { registerCommand } from "@/state/commands";
import { isPluginActive } from "@/state/project";
import { showToolWindow } from "@/state/layout";
import { getSetting } from "@/state/settings";
import { balloonsFor } from "@/keymap/dispatch";
import { t } from "@/i18n";

const showBalloon = balloonsFor("JavaScript Debugger");
const PLUGIN = "forge.js-debugger";
const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";

/** Configuration types Debug starts with js-debug. */
export const JS_DEBUG_TYPES = new Set(["node", "npm", "javascript-debug", "js-attach", "jest", "vitest", "mocha", "playwright", "node-test"]);

registerRunConfigurationType({
  id: "node",
  name: "Node.js",
  description: "Run a JavaScript or TypeScript file with Node.js; Debug starts it under js-debug.",
  icon: Hexagon,
  color: "var(--success)",
  pluginId: PLUGIN,
  interpreter: "node",
  defaultInterpreter: () => getSetting<string>("web.node.interpreter", "") || "node",
  argumentsLabel: "Application parameters",
  debuggable: true,
  coverage: true,
  coverageFields: [
    {
      key: "coverageTool",
      label: "Coverage tool",
      kind: "segmented",
      default: "c8",
      options: [
        { value: "c8", label: "c8" },
        { value: "nyc", label: "nyc" },
      ],
    },
    { key: "c8Package", label: "c8 package", kind: "path", placeholder: "node_modules/c8", visible: (c) => (c.options?.coverageTool ?? "c8") !== "nyc" },
    { key: "nycPackage", label: "nyc package", kind: "path", placeholder: "node_modules/nyc", visible: (c) => c.options?.coverageTool === "nyc" },
  ],
  fields: [{ key: "file", label: "JavaScript file", kind: "path", required: true, placeholder: "src/server.ts" }],
  suggestName: (cfg) => baseName(opt(cfg, "file")),
});

registerRunConfigurationType({
  id: "javascript-debug",
  name: "JavaScript Debug",
  description: "Open a URL in Chrome or Edge; Debug launches the browser under js-debug with breakpoints in your sources.",
  icon: Globe,
  color: "var(--info)",
  pluginId: PLUGIN,
  argumentsLabel: false,
  debuggable: true,
  noRunTarget: true,
  fields: [
    { key: "url", label: "URL", required: true, default: "http://localhost:5173/", placeholder: "http://localhost:5173/" },
    {
      key: "browser",
      label: "Browser",
      kind: "enum",
      default: "",
      options: [
        { value: "", label: "Default (Settings | Node.js)" },
        { value: "chrome", label: "Google Chrome" },
        { value: "edge", label: "Microsoft Edge" },
        { value: "chromium", label: "Chromium" },
      ],
    },
    { key: "browserArguments", label: "Browser arguments", placeholder: "--auto-open-devtools-for-tabs" },
  ],
  suggestName: (cfg) => opt(cfg, "url").replace(/^https?:\/\//, "") || undefined,
  // Run opens the URL in the browser; the browser is found by Rust like for Debug.
  start: async (cfg, request) => {
    const r = await jsDebugApi.request(cfg);
    const browser = String((r.configuration as Record<string, unknown> | undefined)?.runtimeExecutable ?? "");
    return runApi.start({ ...request, configuration: undefined, command: { program: browser, args: [r.program ?? opt(cfg, "url")], cwd: "", env: {} } });
  },
});

registerRunConfigurationType({
  id: "js-attach",
  name: "Attach to Node.js/Chrome",
  description: "Attach the debugger to Node.js started with --inspect, or to Chrome or Edge started with --remote-debugging-port.",
  icon: Plug,
  color: "var(--warning)",
  pluginId: PLUGIN,
  argumentsLabel: false,
  debuggable: true,
  noRunTarget: true,
  fields: [
    {
      key: "debugger",
      label: "Attach to",
      kind: "segmented",
      default: "node",
      options: [
        { value: "node", label: "Node.js" },
        { value: "chrome", label: "Chrome" },
        { value: "edge", label: "Edge" },
      ],
    },
    { key: "host", label: "Host", default: "localhost" },
    { key: "port", label: "Port", placeholder: "9229 (Node.js) or 9222 (browsers)" },
    { key: "remoteRoot", label: "Remote root", placeholder: "/app (the project's folder in a container)" },
  ],
  suggestName: (cfg) => `${opt(cfg, "host") || "localhost"}:${opt(cfg, "port") || (opt(cfg, "debugger") === "node" || !opt(cfg, "debugger") ? "9229" : "9222")}`,
  start: () => Promise.reject(new Error("Attach configurations only debug: use Debug (Shift+F9)")),
});

// ---- js-debug ---------------------------------------------------------------------------

const megabytes = (n: number) => `${(n / 1024 / 1024).toFixed(1)} MB`;

let installing: Promise<boolean> | null = null;

/** Download js-debug after asking; `true` when it is ready. */
async function install(status: JsDebugStatus): Promise<boolean> {
  const r = status.release;
  const ok = await confirmDialog({
    title: "Download JavaScript Debugger",
    message: `Debugging JavaScript and TypeScript uses Microsoft's js-debug (MIT licence). Forge downloads js-debug ${r.version} (${megabytes(r.size)}) from its GitHub release, checks its SHA-256 checksum and keeps it in Forge's data folder.${status.node ? "" : " No Node.js 18 or newer was found: Forge downloads Node.js from nodejs.org as well."}`,
    detail: r.url,
    question: "Download it now?",
    okLabel: "Download",
  });
  if (!ok) return false;
  showBalloon(t("langs.webforge.downloadingJsDebugEllipsis", { version: r.version }), t("langs.webforge.theDebugSessionStartsWhen"), 4000);
  try {
    const done = await jsDebugApi.install();
    await refreshDebugAdapters();
    showBalloon(t("langs.webforge.jsDebugIsReady", { version: done.release.version }), done.server ?? "", 4000);
    return !!done.server;
  } catch (e) {
    showBalloon(t("langs.webforge.cannotDownloadJsDebug"), String(e).replace(/^Error: /, ""), 10000);
    return false;
  }
}

/** Make sure js-debug is installed, asking to download it once. */
export async function ensureJsDebug(): Promise<boolean> {
  const status = await jsDebugApi.status();
  if (status.server) return true;
  installing ??= install(status).finally(() => (installing = null));
  return installing;
}

/** Debug an npm script without js-debug: Node's inspector through NODE_OPTIONS. */
async function inspectorFallback(cfg: RunConfiguration, ctx: DebugLaunchContext): Promise<void> {
  const options = getSetting<string>("web.npm.debugArguments", "--inspect").trim() || "--inspect";
  const existing = cfg.env?.NODE_OPTIONS;
  await ctx.run({ env: { NODE_OPTIONS: existing ? `${existing} ${options}` : options } });
}

registerDebugLauncher({
  id: "js-debug",
  canDebug: (cfg) => JS_DEBUG_TYPES.has(cfg.type) && isPluginActive(PLUGIN),
  async launch(cfg, ctx) {
    if (!(await ensureJsDebug())) {
      if (cfg.type === "npm") await inspectorFallback(cfg, ctx);
      return;
    }
    try {
      const request = await jsDebugApi.request(cfg);
      await refreshDebugAdapters();
      showToolWindow("debug");
      await dapApi.start(request);
    } catch (e) {
      showBalloon(t("common.cannotDebug", { name: cfg.name }), String(e).replace(/^Error: /, ""), 8000);
    }
  },
});

registerCommand({
  id: "jsDebug.install",
  title: "Download JavaScript Debugger (js-debug)",
  category: "Run",
  run: async () => {
    const status = await jsDebugApi.status();
    if (status.server) showBalloon(t("langs.webforge.jsDebugIsInstalled", { value: status.custom ? "" : `${status.release.version} ` }), status.server, 4000);
    else await ensureJsDebug();
  },
});

// ---- test runner actions ----------------------------------------------------------------------

/** Run a package's CLI with Node.js in a temporary "Node.js" configuration. */
function runCli(name: string, file: string, args: string): void {
  void runConfiguration(useTemporary({ name, type: "node", options: { file }, arguments: args }));
}

registerCommand({
  id: "playwright.showReport",
  title: "Show Playwright Report",
  category: "Tools",
  run: () => runCli("Playwright report", "node_modules/@playwright/test/cli.js", "show-report"),
});

registerCommand({
  id: "cypress.open",
  title: "Open Cypress",
  category: "Tools",
  run: () => runCli("Cypress", "node_modules/cypress/bin/cypress", "open"),
});
