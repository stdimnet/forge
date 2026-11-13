/**
 * PHPForge CMS basics (forge.cms): Drupal, Joomla and Shopware. Detection and the status
 * bar icon come from project detection; this feature adds Drupal hook completion, gutter
 * icons and hook docs (`editor.ts`), Tools | Drush… and Tools | Shopware Console… (the
 * Command Line Tools Console, refused in Safe Mode) and Settings | Languages & Frameworks |
 * PHP | Drupal, Joomla and Shopware. File types (`.module` & co. as PHP) come from the
 * plugin manifest; Twig knows Shopware's `sw_*` tags.
 */
import { createComponent } from "solid-js";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { openToolConsole } from "@/features/cliTools/openTool";
import { registerCommand } from "@/state/commands";
import { registerKeymapActions } from "@/keymap/store";
import { cmsInfo, drupalActive, shopwareActive } from "./store";
import "./editor";
import "./cms.css";
import { t } from "@/i18n";

registerSettingsPage({
  id: "lang.php.cms",
  title: t("settings.page.lang.php.cms"),
  group: "Languages & Frameworks",
  parent: "lang.php",
  order: 24,
  keywords: ["drupal", "joomla", "shopware", "drush", "hooks", "cms", "module"],
  component: () => createComponent(SchemaPage, { pageId: "lang.php.cms" }),
});

registerKeymapActions([
  { id: "Cms.RunDrush", title: "Drush…", group: "Plugins/CMS" },
  { id: "Cms.RunShopwareConsole", title: "Shopware Console…", group: "Plugins/CMS" },
]);

registerCommand({
  id: "Cms.RunDrush",
  title: "Drush…",
  category: "Tools",
  run: () => {
    if (!drupalActive()) return;
    void openToolConsole(
      "drush",
      "Drush",
      cmsInfo("drupal")?.console
        ? `Add Drush (${cmsInfo("drupal")!.console}) in Settings | Tools | Command Line Tool Support.`
        : "Drush was not found in vendor/bin or on PATH. Install it with `composer require drush/drush`.",
    );
  },
});

registerCommand({
  id: "Cms.RunShopwareConsole",
  title: "Shopware Console…",
  category: "Tools",
  run: () => {
    if (!shopwareActive()) return;
    void openToolConsole(
      "symfony",
      "The Shopware console",
      "bin/console was not found in the project. Shopware 6 projects run it with PHP; add it in Settings | Tools | Command Line Tool Support.",
      "bin/console",
    );
  },
});
