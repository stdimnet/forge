/**
 * Language servers for many languages: Settings | Languages & Frameworks | Language
 * Servers (status, install, options), project-wide symbol search through
 * every running server, Forge-managed installs and their update check.
 */
import { render } from "solid-js/web";
import { Show } from "solid-js";
import { registerEditorExtension } from "@/editor/extensions";
import { registerSettingsPage } from "@/features/settingsPages";
import { openSettings } from "@/features/settings/dialogState";
import { registerCommand } from "@/state/commands";
import { registerStatusItem } from "@/features/statusItems";
import { InstallDialog } from "./InstallDialog";
import { InstallStatus } from "./InstallStatus";
import { checkUpdates, startUpdateChecks } from "./managed";
import { LanguageServersPage } from "./LanguageServersPage";
import { serverHintPanel } from "./serverHint";
import { closeInstallRequest, installRequest, startLangserverListeners } from "./store";
import { registerSymbolProviders } from "./symbols";
import { isDialogWindow } from "@/shell/window/mode";
import "./langservers.css";
import { t } from "@/i18n";

registerSettingsPage({
  id: "lang.servers",
  title: t("settings.page.lang.servers"),
  group: "Languages & Frameworks",
  order: 5,
  keywords: [
    "LSP",
    "language server",
    "install",
    "typescript",
    "eslint",
    "tailwind",
    "vue",
    "python",
    "pyright",
    "go",
    "gopls",
    "rust-analyzer",
    "clangd",
    "yaml",
    "json",
    "schema",
    "php-lsp",
  ],
  component: LanguageServersPage,
});

registerCommand({
  id: "langservers.settings",
  title: "Language Servers…",
  category: "Languages & Frameworks",
  run: () => openSettings("lang.servers"),
});

registerCommand({
  id: "langservers.checkUpdates",
  title: "Check for Language Server Updates",
  category: "Languages & Frameworks",
  run: () => void checkUpdates(true),
});

registerSymbolProviders();
startLangserverListeners();
// Update checks belong to the main window, not to dialog windows (Settings).
if (typeof document !== "undefined" && !isDialogWindow()) startUpdateChecks();

// Downloads of Forge-managed installs (Node.js, npm packages), like background tasks.
registerStatusItem({ id: "langservers.install", align: "right", order: -30, component: InstallStatus });

// A banner in files that need a server that is not installed, with a button to install it.
registerEditorExtension(({ path }) => serverHintPanel(path));

/** Hosts the install dialog for requests from outside the settings page (the banner). */
function InstallHost() {
  return <Show when={installRequest()}>{(row) => <InstallDialog row={row()} onClose={closeInstallRequest} />}</Show>;
}

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "langservers-overlay";
  document.body.appendChild(host);
  render(InstallHost, host);
}
