/**
 * RustForge in the UI (RustRover's Rust support). Everything is computed in Rust
 * (`src/ipc/rust.ts`, crates `packs/rustforge/*`); this module plugs it into the
 * contribution points:
 *
 * - Settings | Languages & Frameworks | Rust (toolchain, tools, Cargo, tests, rustfmt,
 *   Clippy, rust-analyzer, debugger),
 * - the Cargo tool window (members, targets, features, dependencies; Reload, Run, Debug,
 *   Build, Check, Clean, Update),
 * - run configuration types "Cargo" and "Cargo Test" (cargo test, nextest, doctests,
 *   coverage with cargo-llvm-cov), Debug through LLDB or GDB with rustc's pretty printers
 *   (`rust_debug_request`, `dap_start`),
 * - actions: Reload Cargo Project, Build, Check, Clean, Update, Run Clippy, Install Rust
 *   Toolchain, Download rust-analyzer,
 * - ▶ on `fn main`, rustfmt and Clippy with quick fixes (`editor.ts`).
 */
import { createEffect, createRoot, on } from "solid-js";
import { FlaskConical, Package, Terminal } from "lucide-solid";
import { rustApi, type CargoAction } from "@/ipc/rust";
import type { CommandLine, RunConfiguration } from "@/ipc/run";
import { registerKeymapActions } from "@/keymap";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerToolWindow } from "@/features/registry";
import { refreshDebugAdapters } from "@/features/debug/adapters";
import { dapApi } from "@/features/debug/dapApi";
import { registerDebugLauncher, registerRunConfigurationType } from "@/features/run/types";
import { isDialogWindow } from "@/shell/window/mode";
import { registerCommand } from "@/state/commands";
import { activeTab } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { workspace } from "@/state/workspace";
import { CargoToolWindow } from "./CargoToolWindow";
import { RustSettingsPage } from "./RustSettingsPage";
import { isRustPath, runClippy } from "./editor";
import { downloadAnalyzer, installAnalyzer, installToolchain } from "./install";
import { PLUGIN, refreshCargo, refreshRustStatus, runRustCommand, rustActive, showRustBalloon } from "./state";

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";

// ---- Settings and the Cargo tool window ----------------------------------------------------

registerSettingsPage({
  id: "lang.rust",
  title: "Rust",
  group: "Languages & Frameworks",
  order: 27,
  keywords: ["rust", "cargo", "rustup", "toolchain", "rust-toolchain.toml", "rustfmt", "clippy", "rust-analyzer", "nextest", "llvm-cov", "features", "lldb", "gdb", "sysroot"],
  component: RustSettingsPage,
});

registerToolWindow({
  id: "cargo",
  title: "Cargo",
  icon: Package,
  anchor: "right",
  order: 41,
  component: CargoToolWindow,
  when: () => rustActive(),
});

// ---- Run configurations ------------------------------------------------------------------------

const featureFields = [
  { key: "features", label: "Features", placeholder: "json, discounts (Settings | Rust adds its features)" },
  { key: "allFeatures", label: "Use all features", kind: "bool" as const },
  { key: "noDefaultFeatures", label: "Do not use the default features", kind: "bool" as const },
  {
    key: "profile",
    label: "Profile",
    kind: "enum" as const,
    default: "",
    options: [
      { value: "", label: "Default (dev, test or bench)" },
      { value: "release", label: "release" },
      { value: "dev", label: "dev" },
    ],
  },
  { key: "targetTriple", label: "Target triple", placeholder: "wasm32-unknown-unknown (host when empty)" },
  { key: "toolchain", label: "Toolchain", placeholder: "nightly (Settings | Rust when empty)" },
];

registerRunConfigurationType({
  id: "cargo",
  name: "Cargo",
  description: "Run a cargo command (run, test, bench, build or any other) with a toolchain, package, target, features, profile and target triple.",
  icon: Package,
  color: "var(--warning)",
  pluginId: PLUGIN,
  argumentsLabel: "Program arguments",
  debuggable: true,
  defaults: () => ({ options: { command: "run" } }),
  fields: [
    { key: "command", label: "Command", required: true, placeholder: "run, build --release, test --doc, clippy -- -W clippy::pedantic" },
    { key: "package", label: "Package", placeholder: "shop-cli (-p)" },
    { key: "target", label: "Target", placeholder: "bin:shop, example:report, test:checkout" },
    ...featureFields,
  ],
  suggestName: (cfg) => {
    const target = opt(cfg, "target").split(":").pop();
    const command = opt(cfg, "command").split(/\s+/)[0] || "run";
    return target ? (command === "run" ? target : `${command} ${target}`) : `cargo ${opt(cfg, "command") || "run"}`;
  },
});

const scope = (cfg: RunConfiguration) => opt(cfg, "scope") || "all";
const inScope = (...list: string[]) => (cfg: RunConfiguration) => list.includes(scope(cfg));

registerRunConfigurationType({
  id: "cargo-test",
  name: "Cargo Test",
  description: "Run cargo test or cargo nextest: the workspace, a package, a target, a module, a test or a doctest; benchmarks; coverage with cargo-llvm-cov.",
  icon: FlaskConical,
  color: "var(--success)",
  pluginId: PLUGIN,
  argumentsLabel: "Test binary arguments",
  debuggable: true,
  testRunner: true,
  coverage: true,
  fields: [
    {
      key: "scope",
      label: "Test scope",
      kind: "segmented",
      default: "all",
      options: [
        { value: "all", label: "All" },
        { value: "package", label: "Package" },
        { value: "target", label: "Target" },
        { value: "test", label: "Test" },
        { value: "pattern", label: "Filter" },
      ],
    },
    { key: "package", label: "Package", required: true, placeholder: "shop-core", visible: inScope("package", "target", "test") },
    { key: "target", label: "Target", placeholder: "lib, test:checkout, doc (doctests)", visible: inScope("target", "test", "pattern") },
    { key: "test", label: "Test", required: true, placeholder: "cart::tests::adds_items or cart::", visible: inScope("test", "pattern") },
    { key: "exact", label: "Exact name (--exact)", kind: "bool", visible: inScope("test") },
    {
      key: "kind",
      label: "Run",
      kind: "enum",
      default: "test",
      options: [
        { value: "test", label: "Tests (cargo test)" },
        { value: "bench", label: "Benchmarks (cargo bench)" },
      ],
    },
    {
      key: "runner",
      label: "Test runner",
      kind: "enum",
      default: "auto",
      options: [
        { value: "auto", label: "Settings | Rust" },
        { value: "cargo", label: "cargo test" },
        { value: "nextest", label: "cargo nextest" },
      ],
    },
    ...featureFields,
    { key: "flags", label: "Cargo options", placeholder: "--release --jobs 4" },
  ],
  suggestName: (cfg) => {
    switch (scope(cfg)) {
      case "package":
        return opt(cfg, "package") || undefined;
      case "target":
        return opt(cfg, "target") === "doc" ? `Doctests in ${opt(cfg, "package")}` : opt(cfg, "target").split(":").pop() || opt(cfg, "package") || undefined;
      case "test":
      case "pattern":
        return opt(cfg, "test") || undefined;
      default:
        return "All tests";
    }
  },
});

registerRunConfigurationType({
  id: "cargo-command",
  name: "Cargo",
  icon: Terminal,
  hidden: true,
  fields: [],
  argumentsLabel: false,
  buildCommand: (cfg) => JSON.parse(cfg.options?.commandLine ?? "{}") as CommandLine,
});

// ---- Debugging with LLDB / GDB -------------------------------------------------------------------

registerDebugLauncher({
  id: "rust",
  canDebug: (cfg) => (cfg.type === "cargo" || cfg.type === "cargo-test") && rustActive(),
  async launch(cfg) {
    showRustBalloon(`Building '${cfg.name}' for debugging…`, "", 3000);
    try {
      const request = await rustApi.debugRequest(cfg);
      await refreshDebugAdapters();
      showToolWindow("debug");
      await dapApi.start(request);
    } catch (e) {
      showRustBalloon(`Cannot debug '${cfg.name}'`, String(e).replace(/^Error: /, ""), 10000);
    }
  },
});

// ---- Actions ----------------------------------------------------------------------------------------

async function cargo(action: CargoAction): Promise<void> {
  try {
    await runRustCommand(await rustApi.cargoCommand(action));
  } catch (e) {
    showRustBalloon(`cargo ${action}`, String(e).replace(/^Error: /, ""), 6000);
  }
}

registerKeymapActions([
  { id: "rust.refreshCargo", title: "Reload Cargo Project", group: "Plugins/Rust" },
  { id: "rust.cargoBuild", title: "Build (cargo build)", group: "Plugins/Rust" },
  { id: "rust.cargoCheck", title: "Check (cargo check)", group: "Plugins/Rust" },
  { id: "rust.cargoClean", title: "Clean (cargo clean)", group: "Plugins/Rust" },
  { id: "rust.cargoUpdate", title: "Update Dependencies (cargo update)", group: "Plugins/Rust" },
  { id: "rust.clippy", title: "Run Clippy", group: "Plugins/Rust" },
  { id: "rust.installToolchain", title: "Install Rust Toolchain…", group: "Plugins/Rust" },
  { id: "rust.downloadAnalyzer", title: "Download rust-analyzer…", group: "Plugins/Rust" },
]);

registerCommand({ id: "rust.refreshCargo", title: "Reload Cargo Project", category: "Rust", run: () => void refreshCargo(true) });
registerCommand({ id: "rust.cargoBuild", title: "Build (cargo build)", category: "Rust", run: () => void cargo("build") });
registerCommand({ id: "rust.cargoCheck", title: "Check (cargo check)", category: "Rust", run: () => void cargo("check") });
registerCommand({ id: "rust.cargoClean", title: "Clean (cargo clean)", category: "Rust", run: () => void cargo("clean") });
registerCommand({ id: "rust.cargoUpdate", title: "Update Dependencies (cargo update)", category: "Rust", run: () => void cargo("update") });
registerCommand({
  id: "rust.clippy",
  title: "Run Clippy",
  category: "Rust",
  run: async () => {
    const tab = activeTab();
    await runClippy(tab && isRustPath(tab.path) ? tab.path : undefined);
    showToolWindow("problems");
  },
});
registerCommand({ id: "rust.installToolchain", title: "Install Rust Toolchain…", category: "Rust", run: installToolchain });
registerCommand({ id: "rust.downloadAnalyzer", title: "Download rust-analyzer…", category: "Rust", run: () => void downloadAnalyzer() });

// ---- Startup -----------------------------------------------------------------------------------------

if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(() => {
    createEffect(
      on(
        () => [!!workspace() && rustActive(), workspace()?.root] as const,
        ([active]) => {
          if (!active) return;
          void refreshCargo();
          void refreshRustStatus().then((s) => {
            if (!s) return;
            if (!s.cargo)
              showRustBalloon("No Rust toolchain found", "Install Rust with rustup (rustup.rs), then reload the project.", 10000);
            else if (!s.tools.find((t) => t.tool === "rust-analyzer")?.path)
              showRustBalloon("rust-analyzer is not installed", "Code insight for Rust needs rust-analyzer.", 10000, [
                { label: s.rustup ? "Install with rustup" : "Download…", run: () => void installAnalyzer() },
              ]);
          });
        },
      ),
    );
  });
}

