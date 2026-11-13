/**
 * PHPForge Doctrine plugin (forge.doctrine): Doctrine tool window with the mapped entities,
 * DQL language injection in PHP strings and Settings | Languages & Frameworks | PHP | Doctrine.
 * Active only in projects using doctrine/orm.
 */
import { Boxes } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { registerCommand } from "@/state/commands";
import { registerKeymapActions } from "@/keymap/store";
import { toggleToolWindow } from "@/state/layout";
import { EntitiesActions, EntitiesView } from "./EntitiesView";
import { loadEntities } from "./store";
import "./editor";
import "./doctrine.css";
import { t } from "@/i18n";

registerToolWindow({
  id: "doctrine",
  title: t("toolwindow.doctrine.title"),
  icon: Boxes,
  anchor: "right",
  order: 15,
  component: EntitiesView,
  actions: EntitiesActions,
});

registerSettingsPage({
  id: "lang.php.doctrine",
  title: t("settings.page.lang.php.doctrine"),
  group: "Languages & Frameworks",
  parent: "lang.php",
  order: 30,
  keywords: ["doctrine", "orm", "entity", "dql", "annotations", "mapping", "repository"],
  component: () => <SchemaPage pageId="lang.php.doctrine" />,
});

registerKeymapActions([{ id: "Doctrine.ShowEntities", title: "Doctrine Entities", group: "Plugins/Doctrine" }]);
registerCommand({ id: "Doctrine.ShowEntities", title: "Doctrine Entities", category: "View", run: () => toggleToolWindow("doctrine") });
registerCommand({ id: "doctrine.refresh", title: "Refresh Doctrine Entities", category: "Tools", run: () => void loadEntities(true) });
