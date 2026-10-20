/**
 * PHPForge: PHP language support in the UI — Generate (Alt+Insert), Structure tool window,
 * indexing/PHP version status items, PHP settings page and the index-based hover.
 */
import { createEffect, createRoot, on } from "solid-js";
import { render } from "solid-js/web";
import { keymap } from "@codemirror/view";
import { Prec } from "@codemirror/state";
import { ListTree } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerStatusItem } from "@/features/statusItems";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerEditorExtension } from "@/editor/extensions";
import { registerCommand } from "@/state/commands";
import { toggleToolWindow } from "@/state/layout";
import { workspace } from "@/state/workspace";
import { isPluginActive } from "@/state/project";
import { activeView, viewTracker } from "./editorViews";
import { GenerateOverlay, openGeneratePopup, startGenerate } from "./GenerateOverlay";
import { indexHover } from "./indexHover";
import { StructureActions, StructureView } from "./StructureView";
import { IndexingStatus, PhpVersionStatus, indexWorkspace } from "./status";
import { PhpSettingsPage } from "./PhpSettingsPage";
import "./php.css";

const isPhp = (path: string) => /\.(php|phtml|inc)$/i.test(path) && !path.endsWith(".blade.php");

registerEditorExtension(({ path }) =>
  isPhp(path)
    ? [
        viewTracker(path),
        Prec.high(
          keymap.of([
            { key: "Alt-Insert", mac: "Cmd-n", run: (view) => (void openGeneratePopup(path, view), true) },
            { key: "Ctrl-o", mac: "Ctrl-o", run: (view) => (void startGenerate(path, view, "overrideMethods"), true) },
            { key: "Ctrl-i", mac: "Ctrl-i", run: (view) => (void startGenerate(path, view, "implementMethods"), true) },
          ]),
        ),
        indexHover(path),
      ]
    : null,
);

const withActive = (fn: (path: string, view: NonNullable<ReturnType<typeof activeView>>["view"]) => void) => () => {
  const a = activeView();
  if (a && isPhp(a.path)) fn(a.path, a.view);
};

registerCommand({ id: "php.generate", title: "Generate…", category: "Code", run: withActive((p, v) => void openGeneratePopup(p, v)) });
registerCommand({ id: "php.overrideMethods", title: "Override Methods…", category: "Code", run: withActive((p, v) => void startGenerate(p, v, "overrideMethods")) });
registerCommand({ id: "php.implementMethods", title: "Implement Methods…", category: "Code", run: withActive((p, v) => void startGenerate(p, v, "implementMethods")) });
registerCommand({ id: "php.generatePhpDoc", title: "Generate PHPDoc Comment", category: "Code", run: withActive((p, v) => void startGenerate(p, v, "phpDoc")) });
registerCommand({ id: "php.reindex", title: "Reindex PHP Files", category: "PHP", run: () => void indexWorkspace() });
registerCommand({ id: "ActivateStructureToolWindow", title: "Structure", category: "View", run: () => toggleToolWindow("left", "structure") });

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
  keywords: ["php", "interpreter", "language level", "intelephense", "phpactor", "licence", "license", "include path"],
  component: PhpSettingsPage,
});

if (typeof document !== "undefined") {
  // Overlay for the Generate popup/dialog, independent of the shell layout.
  const host = document.createElement("div");
  host.id = "php-overlay";
  document.body.appendChild(host);
  render(GenerateOverlay, host);

  // Index whenever a PHP project is opened (detection activates the PHP plugin).
  createRoot(() => {
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
