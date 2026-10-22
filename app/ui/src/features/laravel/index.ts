/**
 * PHPForge Laravel plugin (`forge.laravel`): Blade highlighting and completion, Laravel
 * completion and Go to Declaration in PHP and Blade, gutter icons, the Routes tool window,
 * Run Artisan Command… (the Command Line Tools Console with the artisan alias), the
 * "Laravel 12" status item and
 * Settings | Languages & Frameworks | PHP | Laravel. Everything but the Blade syntax mode
 * is active only when the project uses Laravel and the plugin is enabled.
 */
import { StreamLanguage } from "@codemirror/language";
import { Route } from "lucide-solid";
import { registerLanguage } from "@/editor/languages";
import { registerEditorExtension } from "@/editor/extensions";
import { registerToolWindow } from "@/features/registry";
import { registerStatusItem } from "@/features/statusItems";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerCommand } from "@/state/commands";
import { showToolWindow } from "@/state/layout";
import { registerKeymapActions } from "@/keymap";
import { bladeParser } from "./blade";
import { laravelCompletion } from "./completion";
import { laravelGotoExtension, wrapGotoDeclaration } from "./goto";
import { laravelGutter } from "./gutter";
import { RoutesActions, RoutesView } from "./RoutesView";
import { openRunCommand } from "@/features/cliTools/CommandPopup";
import { tools as cliTools } from "@/features/cliTools/store";
import { LaravelSettingsPage, LaravelStatus, showLaravelStatus } from "./status";
import { laravelActive, laravelInfo, laravelSetting, loadRoutes } from "./state";
import "./laravel.css";

registerLanguage({
  id: "blade",
  load: () =>
    StreamLanguage.define(
      bladeParser({
        customDirectives: () => [
          ...(laravelInfo()?.directives ?? []),
          ...laravelSetting<string[]>("laravel.blade.customDirectives", []),
        ],
      }),
    ),
});

const isPhpOrBlade = (path: string) => /\.(php|phtml)$/i.test(path);

registerEditorExtension(({ path }) =>
  isPhpOrBlade(path) ? [laravelCompletion(path), laravelGotoExtension(path), laravelGutter(path)] : null,
);

wrapGotoDeclaration();

registerToolWindow({
  id: "laravel.routes",
  title: "Routes",
  icon: Route,
  anchor: "bottom",
  order: 35,
  component: RoutesView,
  actions: RoutesActions,
  when: laravelActive,
});

registerStatusItem({ id: "framework.laravel", align: "right", order: 80, component: LaravelStatus, when: showLaravelStatus });

registerSettingsPage({
  id: "lang.php.laravel",
  title: "Laravel",
  group: "Languages & Frameworks",
  parent: "lang.php",
  order: 10,
  keywords: ["laravel", "blade", "artisan", "routes", "eloquent", "gutter"],
  component: LaravelSettingsPage,
});

registerKeymapActions([
  { id: "Laravel.RunArtisan", title: "Run Artisan Command…", group: "Laravel" },
  { id: "Laravel.ShowRoutes", title: "Routes", group: "Laravel" },
]);

registerCommand({
  id: "Laravel.RunArtisan",
  title: "Run Artisan Command…",
  category: "Laravel",
  // Tools | Run Command… (Ctrl+Shift+X) with the artisan tool's alias typed in.
  run: () => {
    if (!laravelActive()) return;
    const alias = cliTools().find((t) => t.kind === "artisan" && t.enabled)?.alias ?? "a";
    openRunCommand(`${alias} `);
  },
});
registerCommand({
  id: "Laravel.ShowRoutes",
  title: "Routes",
  category: "Laravel",
  run: () => {
    if (!laravelActive()) return;
    showToolWindow("laravel.routes");
    void loadRoutes();
  },
});
