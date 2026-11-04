/**
 * GoForge in the UI (GoLand's Go support). Everything is computed in Rust (`src/ipc/go.ts`,
 * crates `packs/goforge/*`); this module plugs it into the contribution points:
 *
 * - Settings | Languages & Frameworks | Go (GOROOT with Download…, tools, GOPATH, GOFLAGS,
 *   build tags, environment, formatter, annotators, Delve, gopls),
 * - run configuration types "Go Build", "Go Remote" and the manifest's "Go Test" with
 *   profiles (the Profiler opens the result), Debug through Delve (`go_debug_request`,
 *   `dap_start`; Forge offers to install Delve with the project's Go),
 * - pprof profiles in the shared Profiler tool window (flame graph, call tree, method list),
 * - actions: Sync Dependencies, Download Dependencies, Add Dependency, go vet,
 *   golangci-lint, go generate, Download Go SDK, Open Profile,
 * - go.mod editing, go:generate markers, gofmt / goimports and annotators (`editor.ts`).
 */
import { createEffect, createRoot, on } from "solid-js";
import { Box, Plug } from "lucide-solid";
import { goApi } from "@/ipc/go";
import { runApi, type CommandLine, type RunConfiguration } from "@/ipc/run";
import { registerKeymapActions } from "@/keymap";
import { registerProfilerSource } from "@/features/profiler";
import { registerSettingsPage } from "@/features/settingsPages";
import { confirmDialog } from "@/features/browserCompanion/ConfirmDialog";
import { setInputDialog } from "@/features/git/overlays";
import { refreshDebugAdapters } from "@/features/debug/adapters";
import { dapApi } from "@/features/debug/dapApi";
import { testsOf } from "@/features/run/gutter";
import { testAt } from "@/features/run/tests";
import { useTemporary } from "@/features/run/configStore";
import { launch } from "@/features/run/runStore";
import { baseName, registerDebugLauncher, registerRunConfigurationType, testFrameworkType, type RunHandle } from "@/features/run/types";
import { activeView } from "@/editor/views";
import { isDialogWindow } from "@/shell/window/mode";
import { registerCommand } from "@/state/commands";
import { activeTab } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { settingsState } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { GoSettingsPage } from "./GoSettingsPage";
import { askForProfile, openProfile, ProfilerActions, ProfilerView } from "./ProfilerView";
import { isGoPath, runGenerate, runLint } from "./editor";
import { downloadSdk } from "./sdk";
import { goActive, installTool, PLUGIN, refreshGoStatus, runGoCommand, showGoBalloon } from "./state";
import { t } from "@/i18n";

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";

// ---- Settings ---------------------------------------------------------------------------------

registerSettingsPage({
  id: "lang.go",
  title: t("settings.page.lang.go"),
  group: "Languages & Frameworks",
  order: 26,
  keywords: ["go", "golang", "goroot", "gopath", "goflags", "build tags", "gofmt", "goimports", "gopls", "delve", "dlv", "golangci-lint", "vet", "modules", "proxy", "sdk"],
  component: GoSettingsPage,
});

// ---- Profiler -----------------------------------------------------------------------------------

// The shared Profiler tool window (features/profiler) shows pprof profiles.
registerProfilerSource({ id: "go", label: "pprof", active: goActive, component: ProfilerView, actions: ProfilerActions, order: 20 });

// ---- Run configurations -----------------------------------------------------------------------

registerRunConfigurationType({
  id: "go-build",
  name: "Go Build",
  description: "Build a Go package, folder or files, and run the program (go run) or only build it (go build -o).",
  icon: Box,
  color: "var(--info)",
  pluginId: PLUGIN,
  argumentsLabel: "Program arguments",
  debuggable: true,
  defaults: () => ({ options: { kind: "package", package: "./cmd/", runAfterBuild: "true" } }),
  fields: [
    {
      key: "kind",
      label: "Run kind",
      kind: "segmented",
      default: "package",
      options: [
        { value: "package", label: "Package" },
        { value: "directory", label: "Directory" },
        { value: "file", label: "File" },
      ],
    },
    { key: "package", label: "Package path", required: true, placeholder: "./cmd/ledger or github.com/acme/ledger/cmd/ledger", visible: (c) => (opt(c, "kind") || "package") === "package" },
    { key: "directory", label: "Directory", kind: "path", directory: true, required: true, visible: (c) => opt(c, "kind") === "directory" },
    { key: "files", label: "Files", kind: "text", required: true, placeholder: "cmd/tool/main.go (one per line)", visible: (c) => opt(c, "kind") === "file" },
    { key: "runAfterBuild", label: "Run after build", kind: "bool", default: "true" },
    { key: "output", label: "Output directory or file", kind: "path", placeholder: "bin/ledger (go build -o)", visible: (c) => opt(c, "runAfterBuild") === "false" },
    { key: "tags", label: "Build tags", placeholder: "integration,prod" },
    { key: "flags", label: "Go tool arguments", placeholder: "-race -trimpath" },
  ],
  suggestName: (cfg) => {
    switch (opt(cfg, "kind")) {
      case "directory":
        return baseName(opt(cfg, "directory"));
      case "file":
        return baseName(opt(cfg, "files").split("\n")[0])?.replace(/\.go$/, "");
      default:
        return baseName(opt(cfg, "package").replace(/\/$/, ""));
    }
  },
});

registerRunConfigurationType({
  id: "go-remote",
  name: "Go Remote",
  description: "Attach the debugger to dlv --headless --listen=:2345 --accept-multiclient (Delve 1.20 or newer), e.g. in a container or on another machine.",
  icon: Plug,
  color: "var(--warning)",
  pluginId: PLUGIN,
  argumentsLabel: false,
  debuggable: true,
  noRunTarget: true,
  fields: [
    { key: "host", label: "Host", default: "localhost" },
    { key: "port", label: "Port", default: "2345", required: true },
    { key: "remotePath", label: "Project folder on the remote side", placeholder: "/app (maps breakpoints with substitutePath)" },
  ],
  suggestName: (cfg) => `${opt(cfg, "host") || "localhost"}:${opt(cfg, "port") || "2345"}`,
  start: () => Promise.reject(new Error("Go Remote configurations only debug: use Debug (Shift+F9)")),
});

registerRunConfigurationType({
  id: "go-command",
  name: "Go",
  icon: Box,
  hidden: true,
  fields: [],
  argumentsLabel: false,
  buildCommand: (cfg) => JSON.parse(cfg.options?.commandLine ?? "{}") as CommandLine,
});

/** Profiles written by runs, by run id. */
const profileOf = new Map<string, { path: string; kind: string }>();

/**
 * "Go Test" from the manifest's `testFrameworks` entry, plus profiles: a run with a
 * profile writes it to Forge's cache folder and opens it in the Profiler.
 */
if (typeof document !== "undefined") {
  createRoot(() =>
    createEffect(
      on(
        () => settingsState.plugins.find((p) => p.id === PLUGIN)?.contributions?.testFrameworks?.find((f) => f.id === "go-test"),
        (spec) => {
          if (!spec) return;
          const base = testFrameworkType(spec, PLUGIN);
          registerRunConfigurationType({
            ...base,
            fields: [
              ...base.fields.filter((f) => !["executable", "testRunnerOptions"].includes(f.key)),
              { key: "recursive", label: "Include sub folders", kind: "bool", default: "true", visible: (c) => opt(c, "scope") === "directory" },
            ],
            argumentsLabel: "Program arguments (-args)",
            start: async (cfg, request) => {
              const kind = opt(cfg, "profile");
              if (!kind || kind === "none") return runApi.start(request);
              const path = await goApi.profilePath(`${cfg.name}-${kind}`);
              const withFile = { ...cfg, options: { ...cfg.options, profileFile: path } };
              const started = await runApi.start({ ...request, configuration: withFile });
              profileOf.set(started.runId, { path, kind });
              return started;
            },
            afterStart: (cfg: RunConfiguration, handle: RunHandle) => {
              const p = profileOf.get(handle.started.runId);
              if (!p) return;
              void handle.exited.then(() => {
                profileOf.delete(handle.started.runId);
                void openProfile(p.path, `${cfg.name} (${p.kind})`);
              });
            },
          });
        },
      ),
    ),
  );
}

// ---- Debugging with Delve -----------------------------------------------------------------------

const DELVE_TYPES = new Set(["go-build", "go-test", "go-remote"]);
let installing: Promise<boolean> | null = null;

/** Make sure Delve is installed, offering to install it with the project's Go once. */
async function ensureDelve(): Promise<boolean> {
  const status = await goApi.delveStatus();
  if (status.path) return true;
  if (!status.install) return false;
  installing ??= (async () => {
    const ok = await confirmDialog({
      title: "Install Delve",
      message: "Debugging Go uses Delve (MIT licence), which is not installed. Forge installs it with your Go toolchain (go install, verified by the Go checksum database) into Forge's tools folder.",
      detail: `go install ${status.install}`,
      question: "Install it now?",
      okLabel: "Install",
    });
    return ok && (await installTool("dlv"));
  })().finally(() => (installing = null));
  return installing;
}

registerDebugLauncher({
  id: "delve",
  canDebug: (cfg) => DELVE_TYPES.has(cfg.type) && goActive(),
  async launch(cfg) {
    if (!(await ensureDelve())) return;
    try {
      const request = await goApi.debugRequest(cfg);
      await refreshDebugAdapters();
      showToolWindow("debug");
      await dapApi.start(request);
    } catch (e) {
      showGoBalloon(`Cannot debug '${cfg.name}'`, String(e).replace(/^Error: /, ""), 8000);
    }
  },
});

// ---- Actions ------------------------------------------------------------------------------------

async function moduleCommand(action: "tidy" | "download" | "get", arg?: string): Promise<void> {
  try {
    const cmd = await goApi.moduleCommand(action, arg);
    await runGoCommand(cmd);
  } catch (e) {
    showGoBalloon("Go Modules", String(e).replace(/^Error: /, ""), 6000);
  }
}

function addDependency(): void {
  setInputDialog({
    title: "Add Dependency",
    label: "Module path, optionally with @version (go get):",
    value: "github.com/",
    okLabel: "Add",
    onOk: (value) => moduleCommand("get", value.trim()),
  });
}

/** Run the test at the caret with the CPU profiler. */
async function profileAtCaret(kind: "cpu" | "mem"): Promise<void> {
  const view = activeView();
  const path = activeTab()?.path ?? "";
  if (!view || !path.endsWith("_test.go")) {
    showGoBalloon("Profile", "Put the caret in a Go test or benchmark.", 4000);
    return;
  }
  const tests = testsOf(view.state);
  const line = view.state.doc.lineAt(view.state.selection.main.head).number - 1;
  const target = testAt(tests.targets, line) ?? tests.targets.find((t) => t.kind !== "suite");
  if (!target) return;
  const cfg = target.configuration as RunConfiguration;
  await launch(useTemporary({ ...cfg, name: `${cfg.name} (${kind} profile)`, options: { ...cfg.options, profile: kind } }));
}

registerKeymapActions([
  { id: "go.syncDependencies", title: "Sync Dependencies (go mod tidy)", group: "Plugins/Go" },
  { id: "go.downloadDependencies", title: "Download Dependencies (go mod download)", group: "Plugins/Go" },
  { id: "go.addDependency", title: "Add Dependency…", group: "Plugins/Go" },
  { id: "go.vet", title: "Run go vet", group: "Plugins/Go" },
  { id: "go.golangciLint", title: "Run golangci-lint", group: "Plugins/Go" },
  { id: "go.generate", title: "Run go generate for File", group: "Plugins/Go" },
  { id: "go.downloadSdk", title: "Download Go SDK…", group: "Plugins/Go" },
  { id: "go.openProfile", title: "Open Profile (pprof)…", group: "Plugins/Go" },
  { id: "go.profileCpu", title: "Profile Test with CPU Profiler", group: "Plugins/Go" },
  { id: "go.profileMemory", title: "Profile Test with Memory Profiler", group: "Plugins/Go" },
]);

registerCommand({ id: "go.syncDependencies", title: "Sync Dependencies (go mod tidy)", category: "Go", run: () => void moduleCommand("tidy") });
registerCommand({ id: "go.downloadDependencies", title: "Download Dependencies (go mod download)", category: "Go", run: () => void moduleCommand("download") });
registerCommand({ id: "go.addDependency", title: "Add Dependency…", category: "Go", run: addDependency });
registerCommand({
  id: "go.vet",
  title: "Run go vet",
  category: "Go",
  run: async () => {
    await runLint("vet");
    showToolWindow("problems");
  },
});
registerCommand({
  id: "go.golangciLint",
  title: "Run golangci-lint",
  category: "Go",
  run: async () => {
    await runLint("golangci");
    showToolWindow("problems");
  },
});
registerCommand({
  id: "go.generate",
  title: "Run go generate for File",
  category: "Go",
  run: () => {
    const tab = activeTab();
    if (tab && isGoPath(tab.path)) void runGenerate(tab.path, tab.content);
    else showGoBalloon("go generate", "Open a Go file first.", 4000);
  },
});
registerCommand({ id: "go.downloadSdk", title: "Download Go SDK…", category: "Go", run: () => void downloadSdk() });
registerCommand({ id: "go.openProfile", title: "Open Profile (pprof)…", category: "Go", run: askForProfile });
registerCommand({ id: "go.profileCpu", title: "Profile Test with CPU Profiler", category: "Go", run: () => void profileAtCaret("cpu") });
registerCommand({ id: "go.profileMemory", title: "Profile Test with Memory Profiler", category: "Go", run: () => void profileAtCaret("mem") });

// ---- Startup ------------------------------------------------------------------------------------

if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(() => {
    createEffect(
      on(
        () => [!!workspace() && goActive(), workspace()?.root] as const,
        ([on]) => {
          if (!on) return;
          void refreshGoStatus().then((s) => {
            if (s && !s.sdk)
              showGoBalloon("No Go SDK found", "Install Go, choose an SDK in Settings | Go, or download one.", 10000, [
                { label: "Download Go SDK…", run: () => void downloadSdk() },
              ]);
          });
        },
      ),
    );
  });
}

// Screenshot scenarios open the demo profile directly.
if (import.meta.env.DEV && typeof window !== "undefined") {
  (window as unknown as { __forgeOpenProfile?: unknown }).__forgeOpenProfile = openProfile;
}
