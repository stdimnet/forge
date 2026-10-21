/**
 * Language servers for many languages: Settings | Languages & Frameworks | Language
 * Servers (status, install, options), project-wide symbol search through
 * every running server, and the Intelephense licence UI used by the PHP page.
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
import { IntelephenseConsent } from "./IntelephenseConsent";
import { answerConsent, checkUpdates, consentRequest, installIntelephense, startAutoInstall } from "./managed";
import { LanguageServersPage } from "./LanguageServersPage";
import { serverHintPanel } from "./serverHint";
import { closeInstallRequest, installRequest, startLangserverListeners } from "./store";
import { registerSymbolProviders } from "./symbols";
import "./langservers.css";

export { IntelephenseLicence } from "./Licence";

registerSettingsPage({
  id: "lang.servers",
  title: "Language Servers",
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
    "intelephense",
    "licence",
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
  id: "langservers.installIntelephense",
  title: "Install Intelephense",
  category: "PHP",
  run: () => void installIntelephense(),
});

registerCommand({
  id: "langservers.checkUpdates",
  title: "Check for Language Server Updates",
  category: "Languages & Frameworks",
  run: () => void checkUpdates(true),
});

registerSymbolProviders();
startLangserverListeners();
startAutoInstall();

// Downloads of Forge-managed installs (Node.js, npm packages), like background tasks.
registerStatusItem({ id: "langservers.install", align: "right", order: -30, component: InstallStatus });

// A banner in files that need a server that is not installed, with a button to install it.
registerEditorExtension(({ path }) => serverHintPanel(path));

/** Hosts the install dialog for requests from outside the settings page (the banner). */
function InstallHost() {
  return (
    <>
      <Show when={installRequest()}>{(row) => <InstallDialog row={row()} onClose={closeInstallRequest} />}</Show>
      <Show when={consentRequest()}>
        <IntelephenseConsent onAnswer={answerConsent} />
      </Show>
    </>
  );
}

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "langservers-overlay";
  document.body.appendChild(host);
  render(InstallHost, host);
}
