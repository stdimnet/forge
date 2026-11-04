/**
 * PHPForge WordPress plugin (forge.wordpress): hook completion, navigation, gutter icons and
 * Find Usages (`editor.ts`), the Yoda condition hint, the WordPress tool window with the
 * project's hooks, Tools | WP-CLI… and Settings | Languages & Frameworks | PHP | WordPress.
 * Active only in WordPress projects (a site, a Composer install, a plugin or a theme).
 */
import { Webhook } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { registerCommand } from "@/state/commands";
import { registerKeymapActions } from "@/keymap/store";
import { toggleToolWindow } from "@/state/layout";
import { HooksActions, HooksView } from "./HooksView";
import { installStubs, loadWordPress, wordpressActive } from "./store";
import { runWpCli } from "./wpcli";
import "./editor";
import "./wordpress.css";
import { t } from "@/i18n";

registerToolWindow({
  id: "wordpress",
  title: t("toolwindow.wordpress.title"),
  icon: Webhook,
  anchor: "bottom",
  order: 36,
  component: HooksView,
  actions: HooksActions,
  when: wordpressActive,
});

registerSettingsPage({
  id: "lang.php.wordpress",
  title: t("settings.page.lang.php.wordpress"),
  group: "Languages & Frameworks",
  parent: "lang.php",
  order: 22,
  keywords: ["wordpress", "hooks", "actions", "filters", "wp-cli", "yoda", "stubs", "plugin", "theme"],
  component: () => <SchemaPage pageId="lang.php.wordpress" />,
});

registerKeymapActions([
  { id: "WordPress.RunWpCli", title: "WP-CLI…", group: "Plugins/WordPress" },
  { id: "WordPress.ShowHooks", title: "WordPress Hooks", group: "Plugins/WordPress" },
  { id: "WordPress.InstallStubs", title: "Install WordPress Stubs…", group: "Plugins/WordPress" },
]);

const when = (run: () => void) => () => {
  if (wordpressActive()) run();
};
registerCommand({ id: "WordPress.RunWpCli", title: "WP-CLI…", category: "Tools", run: when(runWpCli) });
registerCommand({ id: "WordPress.ShowHooks", title: "WordPress Hooks", category: "View", run: when(() => toggleToolWindow("wordpress")) });
registerCommand({ id: "WordPress.InstallStubs", title: "Install WordPress Stubs…", category: "Tools", run: when(() => void installStubs()) });
registerCommand({ id: "wordpress.refresh", title: "Refresh WordPress Hook Index", category: "Tools", run: when(() => void loadWordPress(true)) });
