/**
 * PHPForge Symfony plugin (forge.symfony): routes/services tool window, console commands,
 * completion and navigation for Symfony references in PHP, Twig and YAML, line markers,
 * the "Symfony 7.2 · dev" status widget and Settings | Languages & Frameworks | PHP | Symfony.
 * Active only in Symfony projects.
 */
import { Show } from "solid-js";
import { render } from "solid-js/web";
import { Route } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerStatusItem } from "@/features/statusItems";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { claimFrameworkStatus } from "@/features/project-type";
import { registerCommand } from "@/state/commands";
import { registerKeymapActions } from "@/keymap/store";
import { toggleToolWindow } from "@/state/layout";
import { ConsolePopup, consolePopupOpen, openConsolePopup } from "./ConsolePopup";
import { SymfonyActions, SymfonyView } from "./SymfonyView";
import { SymfonyStatus } from "./status";
import { loadSymfony } from "./store";
import "./editor";
import "./translations";
import "./symfony.css";
import { t } from "@/i18n";

registerToolWindow({
  id: "symfony",
  title: t("toolwindow.symfony.title"),
  icon: Route,
  anchor: "bottom",
  order: 35,
  component: SymfonyView,
  actions: SymfonyActions,
});

registerStatusItem({ id: "symfony.status", align: "right", order: 80, component: SymfonyStatus });
claimFrameworkStatus("symfony");

registerSettingsPage({
  id: "lang.php.symfony",
  title: t("settings.page.lang.php.symfony"),
  group: "Languages & Frameworks",
  parent: "lang.php",
  order: 20,
  keywords: ["symfony", "container", "services", "routes", "twig", "console", "environment", "kernel"],
  component: () => <SchemaPage pageId="lang.php.symfony" />,
});

registerKeymapActions([
  { id: "Symfony.RunConsoleCommand", title: "Run Console Command…", group: "Plugins/Symfony" },
  { id: "Symfony.ShowRoutes", title: "Symfony Routes", group: "Plugins/Symfony" },
]);
registerCommand({ id: "Symfony.RunConsoleCommand", title: "Run Console Command…", category: "Tools", run: () => openConsolePopup() });
registerCommand({ id: "symfony.runConsoleCommand", title: "Run Console Command…", category: "Tools", run: () => openConsolePopup() });
registerCommand({ id: "Symfony.ShowRoutes", title: "Symfony Routes", category: "View", run: () => toggleToolWindow("symfony") });
registerCommand({ id: "symfony.refresh", title: "Refresh Symfony Index", category: "Tools", run: () => void loadSymfony(true) });

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "symfony-overlay";
  document.body.appendChild(host);
  render(
    () => (
      <Show when={consolePopupOpen()}>
        <ConsolePopup />
      </Show>
    ),
    host,
  );
}
