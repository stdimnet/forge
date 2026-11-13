/**
 * Godot (GDScript and C#), like Rider's Godot support (logic in Rust:
 * `packs/dotnetforge/gamedev`, IPC `godot_*`):
 *
 * - the **Godot** run configuration type (the project, the current scene or a chosen one),
 *   Run Current Scene, Open in Godot Editor,
 * - Debug: GDScript through the Godot editor's debug adapter (port 6006; the editor runs the
 *   game), C# with netcoredbg launching Godot after `dotnet build`,
 * - GDScript code insight from the Godot editor's language server (port 6005, registered in
 *   Rust; a balloon explains when the editor is not running),
 * - the **GdUnit4** test configuration type.
 */
import { createEffect, createResource, createRoot, on } from "solid-js";
import { FlaskConical, Gamepad2 } from "lucide-solid";
import { godotApi, type GodotStatus } from "@/ipc/gamedev";
import { balloonsFor, registerKeymapActions } from "@/keymap";
import { action, registerMainMenuEntries } from "@/features/mainMenu/model";
import { refreshDebugAdapters } from "@/features/debug/adapters";
import { dapApi } from "@/features/debug/dapApi";
import { useTemporary } from "@/features/run/configStore";
import { launch } from "@/features/run/runStore";
import { registerDebugLauncher, registerRunConfigurationType } from "@/features/run/types";
import { registerTestFileMatcher } from "@/features/run/frameworkFiles";
import { isDialogWindow } from "@/shell/window/mode";
import { registerCommand } from "@/state/commands";
import { activeTab } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { isPluginActive, usesFramework } from "@/state/project";
import { getSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { errorText, runCommandLine } from "./shared";
import { t } from "@/i18n";

export const GODOT_PLUGIN = "forge.godot";
export const godotActive = () => isPluginActive(GODOT_PLUGIN) && usesFramework("godot");
const showBalloon = balloonsFor("Godot");

const [status, { refetch }] = createRoot(() =>
  createResource(
    () => (godotActive() ? (workspace()?.root ?? null) : null),
    () => godotApi.status().catch(() => null as GodotStatus | null),
  ),
);
export const godotStatus = status;
export const refreshGodot = () => refetch();

/** Open the project in the Godot editor (it serves GDScript code insight and debugging). */
export async function openGodotEditor(): Promise<void> {
  try {
    const cmd = await godotApi.command("openEditor");
    void runCommandLine(cmd, "Godot Editor");
    setTimeout(() => void refetch(), 6000);
  } catch (e) {
    showBalloon(t("langs.gamedev.cannotOpenTheGodotEditor"), errorText(e), 8000);
  }
}

const editorAction = () => [{ label: t("langs.gamedev.openInGodotEditor"), run: () => void openGodotEditor() }];

// ---- Run configurations -------------------------------------------------------------------------

const opt = (cfg: { options?: Record<string, string> }, key: string) => cfg.options?.[key] ?? "";
const sceneOptions = () => (status()?.scenes ?? []).map((s) => ({ value: s, label: s }));

registerRunConfigurationType({
  id: "godot",
  name: "Godot",
  description: t("langs.gamedev.runAGodotProjectOr"),
  icon: Gamepad2,
  color: "var(--syn-type)",
  pluginId: GODOT_PLUGIN,
  argumentsLabel: "Program arguments",
  debuggable: true,
  noRunTarget: true,
  defaults: () => ({ options: { target: "project" } }),
  fields: [
    {
      key: "target",
      label: t("common.run2"),
      kind: "segmented",
      default: "project",
      options: [
        { value: "project", label: t("langs.gamedev.projectMainScene") },
        { value: "scene", label: t("langs.gamedev.scene") },
      ],
    },
    { key: "scene", label: t("langs.gamedev.scene"), kind: "enum", required: true, options: sceneOptions, visible: (cfg) => opt(cfg, "target") === "scene" },
    { key: "headless", label: t("langs.gamedev.headlessHeadless"), kind: "bool" },
    { key: "godotArgs", label: t("langs.gamedev.godotOptions"), placeholder: "--debug-collisions --verbose" },
    {
      key: "debugger",
      label: t("langs.gamedev.debugWith"),
      kind: "enum",
      default: "auto",
      options: [
        { value: "auto", label: t("langs.gamedev.settingsGodot") },
        { value: "gdscript", label: t("langs.gamedev.theGodotEditorGdscript") },
        { value: "csharp", label: "netcoredbg (C#)" },
      ],
    },
  ],
  suggestName: (cfg) => (opt(cfg, "target") === "scene" ? opt(cfg, "scene").split("/").pop()?.replace(/\.t?scn$/, "") : status()?.project?.name || "Godot"),
});

registerRunConfigurationType({
  id: "gdunit4",
  name: "GdUnit4",
  description: t("langs.gamedev.runGdunit4TestSuitesHeadless"),
  icon: FlaskConical,
  color: "var(--success)",
  pluginId: GODOT_PLUGIN,
  argumentsLabel: false,
  testRunner: true,
  noRunTarget: true,
  fields: [
    {
      key: "scope",
      label: t("common.testScope"),
      kind: "segmented",
      default: "all",
      options: [
        { value: "all", label: t("langs.gamedev.allTest") },
        { value: "directory", label: t("langs.gamedev.folder") },
        { value: "file", label: t("langs.gamedev.suite") },
      ],
    },
    { key: "directory", label: t("langs.gamedev.folder"), kind: "path", placeholder: "test", visible: (cfg) => opt(cfg, "scope") === "directory" },
    { key: "file", label: t("langs.gamedev.suite"), kind: "path", required: true, placeholder: "test/player_test.gd", visible: (cfg) => opt(cfg, "scope") === "file" },
    { key: "flags", label: t("langs.gamedev.gdunit4Options"), placeholder: "--verbose" },
  ],
  suggestName: (cfg) => (opt(cfg, "scope") === "file" ? opt(cfg, "file").split("/").pop()?.replace(/\.gd$/, "") : "GdUnit4 Tests"),
});

// GdUnit4 suites get the test gutter (discovery in Rust: `godot::gdunit`).
registerTestFileMatcher((path) => godotActive() && /\.gd$/i.test(path));

registerDebugLauncher({
  id: "godot",
  canDebug: (cfg) => cfg.type === "godot" && godotActive(),
  async launch(cfg) {
    try {
      await refreshDebugAdapters();
      const own = opt(cfg, "debugger");
      const pick = own && own !== "auto" ? own : getSetting<string>("godot.debugger", "auto");
      const csharp = pick === "csharp" || (pick === "auto" && !!status()?.project?.csharp);
      if (csharp) showBalloon(t("langs.gamedev.buildingForDebuggingEllipsis", { name: cfg.name }), "", 3000);
      const request = await godotApi.debugRequest(cfg);
      showToolWindow("debug");
      await dapApi.start(request);
    } catch (e) {
      const text = errorText(e);
      showBalloon(t("common.cannotDebug", { name: cfg.name }), text, 12000, /Godot editor/.test(text) ? editorAction() : []);
    }
  },
});

// ---- Actions ------------------------------------------------------------------------------------

const needsGodot = (run: () => void) => () => {
  if (!godotActive()) {
    showBalloon(t("langs.gamedev.notAGodotProject"), t("langs.gamedev.openAFolderWithProject"), 5000);
    return;
  }
  run();
};

async function runScene(): Promise<void> {
  const path = activeTab()?.path ?? "";
  if (!/\.t?scn$/i.test(path)) {
    showBalloon(t("langs.gamedev.runCurrentScene"), t("langs.gamedev.openASceneTscnIn"), 5000);
    return;
  }
  const name = path.split("/").pop()!.replace(/\.t?scn$/i, "");
  await launch(useTemporary({ name, type: "godot", options: { target: "scene", scene: path } }));
}

registerKeymapActions([
  { id: "godot.openEditor", title: "Open in Godot Editor", group: "Plugins/Godot" },
  { id: "godot.runScene", title: "Run Current Scene", group: "Plugins/Godot" },
  { id: "godot.runProject", title: "Run Godot Project", group: "Plugins/Godot" },
]);
registerCommand({ id: "godot.openEditor", title: "Open in Godot Editor", category: "Godot", run: needsGodot(() => void openGodotEditor()) });
registerCommand({ id: "godot.runScene", title: "Run Current Scene", category: "Godot", run: needsGodot(() => void runScene()) });
registerCommand({
  id: "godot.runProject",
  title: "Run Godot Project",
  category: "Godot",
  run: needsGodot(() => void launch(useTemporary({ name: status()?.project?.name || "Godot", type: "godot", options: { target: "project" } }))),
});
registerMainMenuEntries(
  "_Tools/_Godot",
  [action("godot.openEditor", "_Open in Godot Editor"), action("godot.runProject", "_Run Godot Project"), action("godot.runScene", "Run _Current Scene")],
  { when: godotActive },
);

// ---- Startup: the GDScript language server needs the editor -------------------------------------------

if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(() => {
    let told = false;
    createEffect(on(() => workspace()?.root, () => (told = false)));
    createEffect(
      on(
        () => [godotActive(), activeTab()?.path ?? ""] as const,
        ([active, path]) => {
          if (!active || told || !/\.gd$/i.test(path) || !getSetting<boolean>("godot.lsp.enabled", true)) return;
          void Promise.resolve(refetch()).then((s) => {
            if (!s || s.remote || s.lspListening || told) return;
            told = true;
            showBalloon(
              t("langs.gamedev.gdscriptCodeInsightNeedsThe"),
              t("langs.gamedev.godotServesCompletionNavigationAnd", { host: s.host, lspPort: s.lspPort }),
              12000,
              s.godot ? editorAction() : [],
            );
          });
        },
      ),
    );
  });
}
