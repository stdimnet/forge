/**
 * Composer (plugin `forge.composer`), like PHPStorm: Tools | Composer (Add Dependency…,
 * Install, Update, Validate, Show Outdated, Audit), the composer.json editor (toolbar,
 * outdated packages, completion, hover), the Manage Composer Dependencies dialog and
 * Settings | PHP | Composer. Audit runs `composer audit` through the vulnerable dependency
 * check, which shows it in the Vulnerable Dependencies tab of the Problems tool window.
 * Logic lives in Rust (`packs/phpforge/composer`, `commands/composer.rs`).
 */
import { render } from "solid-js/web";
import { Package } from "lucide-solid";
import { registerEditorExtension } from "@/editor/extensions";
import { registerKeymapActions, balloonsFor } from "@/keymap";
import { registerRunConfigurationType } from "@/features/run/types";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerCommand } from "@/state/commands";
import { isDialogWindow } from "@/shell/window/mode";
import { runCheck } from "@/features/vulnerabilities/store";
import { ComposerSettingsPage } from "./ComposerSettingsPage";
import { DependenciesDialogHost, openDependenciesDialog } from "./DependenciesDialog";
import { composerEditor } from "./editor";
import { composerActive, runComposer } from "./state";
import "./composer.css";
import { t } from "@/i18n";

const showBalloon = balloonsFor("Composer");

// `composer <command>` runs of the toolbar and Tools | Composer; built by the backend
// (`forge_process` kind `composer-command`), not offered in Run/Debug Configurations.
registerRunConfigurationType({
  id: "composer-command",
  name: "Composer",
  description: "A Composer command.",
  icon: Package,
  color: "var(--syn-function)",
  pluginId: "forge.composer",
  argumentsLabel: "Arguments",
  fields: [{ key: "command", label: "Command", required: true, placeholder: "install" }],
  hidden: true,
});

const ACTIONS: { id: string; title: string; run: () => unknown }[] = [
  { id: "Composer.AddDependency", title: "Add Dependency…", run: openDependenciesDialog },
  { id: "Composer.Install", title: "Install", run: () => runComposer("install") },
  { id: "Composer.Update", title: "Update", run: () => runComposer("update") },
  { id: "Composer.Validate", title: "Validate", run: () => runComposer("validate") },
  { id: "Composer.ShowOutdated", title: "Show Outdated", run: () => runComposer("outdated --direct") },
  // `composer audit` runs with the vulnerable dependency check (features/vulnerabilities).
  { id: "Composer.Audit", title: "Audit", run: () => runCheck({ force: true, tools: ["composer"], reveal: true }) },
];

registerKeymapActions(ACTIONS.map((a) => ({ id: a.id, title: `Composer: ${a.title}`, group: "Composer" })));

for (const a of ACTIONS) {
  registerCommand({
    id: a.id,
    title: `Composer: ${a.title}`,
    category: "Composer",
    run: () => {
      if (!composerActive()) {
        showBalloon("Composer is not used in this project", "Open a project with a composer.json, or enable the Composer plugin.");
        return;
      }
      void a.run();
    },
  });
}

registerEditorExtension(({ path }) => composerEditor(path));

registerSettingsPage({
  id: "lang.php.composer",
  title: t("settings.page.lang.php.composer"),
  group: "Languages & Frameworks",
  parent: "lang.php",
  order: 0,
  keywords: ["composer.phar", "composer.json", "packagist", "dependencies", "outdated", "audit"],
  component: ComposerSettingsPage,
});

if (typeof document !== "undefined" && !isDialogWindow()) {
  const host = document.createElement("div");
  host.id = "composer-overlay";
  document.body.appendChild(host);
  render(DependenciesDialogHost, host);
}
