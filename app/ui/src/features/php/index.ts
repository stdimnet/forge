/**
 * PHPForge: PHP language support in the UI — Generate (Alt+Insert), Structure tool window,
 * indexing/PHP version status items, PHP settings page and the index-based hover.
 */
import { createEffect, createRoot, on } from "solid-js";
import { isDialogWindow } from "@/shell/window/mode";
import { render } from "solid-js/web";
import { ListTree } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerStatusItem } from "@/features/statusItems";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerEditorExtension } from "@/editor/extensions";
import { isLibraryPath } from "@/ipc/codeintel";
import { registerCommand } from "@/state/commands";
import { toggleToolWindow } from "@/state/layout";
import { workspace } from "@/state/workspace";
import { isPluginActive } from "@/state/project";
import { lspClaimsHover } from "@/features/lsp/hover";
import { registerFormatPostProcessor } from "@/features/lsp/format";
import { phpApi } from "./api";
import { activeView, viewFor, viewTracker } from "./editorViews";
import { GenerateOverlay, openGenerateItemsPopup, openGeneratePopup, startGenerate } from "./GenerateOverlay";
import { indexHover, setOtherHoverProvider } from "./indexHover";
import { registerPhpSearchProviders } from "./search";
import { migrateLegacyPhpSettings } from "./settings";
import { StructureActions, StructureView } from "./StructureView";
import { IndexingStatus, PhpVersionStatus, indexWorkspace } from "./status";
import { PhpSettingsPage } from "./PhpSettingsPage";
import { phpMetaEditor } from "./meta";
import "./libraries";
import "./sourceRoots";
import "./php.css";

const isPhp = (path: string) =>
  /\.(php|phtml|inc)$/i.test(path) && !path.endsWith(".blade.php") && !isLibraryPath(path);

registerEditorExtension(({ path }) =>
  isPhp(path)
    ? [
        // Alt+Insert, Ctrl+O and Ctrl+I reach the `php.*` commands through the keymap
        // (Generate / OverrideMethods / ImplementMethods aliases in forge-keymap).
        viewTracker(path),
        indexHover(path),
        // `.phpstorm.meta.php` and JetBrains attributes: inspections and completion.
        phpMetaEditor(path),
      ]
    : null,
);

// One hover only: the language server's (docs or problems) wins; the index hover fills in
// where the server has nothing (not running, vendor-less project, unknown symbol).
setOtherHoverProvider(async (path, pos) => {
  const view = viewFor(path);
  return !!view && (await lspClaimsHover(path, view, pos));
});

registerPhpSearchProviders();
migrateLegacyPhpSettings();

const withActive = (fn: (path: string, view: NonNullable<ReturnType<typeof activeView>>["view"]) => void) => () => {
  const a = activeView();
  if (a && isPhp(a.path)) fn(a.path, a.view);
};

registerCommand({
  id: "php.generate",
  title: "Generate…",
  category: "Code",
  run: () => {
    const a = activeView();
    if (!a) return;
    // Other languages get the entries other features add (Generate Documentation with AI...).
    if (isPhp(a.path)) void openGeneratePopup(a.path, a.view);
    else openGenerateItemsPopup(a.path, a.view);
  },
});
registerCommand({ id: "php.overrideMethods", title: "Override Methods…", category: "Code", run: withActive((p, v) => void startGenerate(p, v, "overrideMethods")) });
registerCommand({ id: "php.implementMethods", title: "Implement Methods…", category: "Code", run: withActive((p, v) => void startGenerate(p, v, "implementMethods")) });
registerCommand({ id: "php.generatePhpDoc", title: "Generate PHPDoc Comment", category: "Code", run: withActive((p, v) => void startGenerate(p, v, "phpDoc")) });
registerCommand({ id: "php.reindex", title: "Reindex PHP Files", category: "PHP", run: () => void indexWorkspace() });
registerCommand({ id: "ActivateStructureToolWindow", title: "Structure", category: "View", run: () => toggleToolWindow("structure") });

registerToolWindow({
  id: "structure",
  title: "Structure",
  icon: ListTree,
  anchor: "left",
  order: 30,
  shortcut: "Alt+7",
  component: StructureView,
  actions: StructureActions,
});

registerStatusItem({ id: "php.indexing", align: "right", order: -10, component: IndexingStatus });
registerStatusItem({ id: "php.version", align: "right", order: 90, component: PhpVersionStatus });

registerSettingsPage({
  id: "lang.php",
  title: "PHP",
  group: "Languages & Frameworks",
  order: 10,
  keywords: ["php", "interpreter", "language level", "php-lsp", "language server", "include path"],
  component: PhpSettingsPage,
});

if (typeof document !== "undefined") {
  // Overlay for the Generate popup/dialog, independent of the shell layout.
  const host = document.createElement("div");
  host.id = "php-overlay";
  document.body.appendChild(host);
  render(GenerateOverlay, host);

  // Index whenever a PHP project is opened (detection activates the PHP plugin); dialog
  // windows only show dialogs.
  if (!isDialogWindow()) createRoot(() => {
    createEffect(
      on(
        () => !!workspace() && isPluginActive("forge.php"),
        (active) => {
          if (active) void indexWorkspace();
        },
      ),
    );
  });
}

// Reformat Code: after the language server, the blank lines of Settings | Editor | Code
// Style | PHP (computed in Rust on the buffer).
registerFormatPostProcessor(async (path, text) => {
  if (!/\.(php|phtml|inc)$/i.test(path) || path.endsWith(".blade.php") || !isPluginActive("forge.php")) return text;
  return (await phpApi.tidyBlankLines(text)) ?? text;
});
