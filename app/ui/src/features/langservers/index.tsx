/**
 * Language servers for many languages: Settings | Languages & Frameworks | Language
 * Servers (status, install, options), the Lua page, project-wide symbol search through
 * every running server, and the Intelephense licence UI used by the PHP page.
 */
import { registerSettingsPage } from "@/features/settingsPages";
import { openSettings } from "@/features/settings/dialogState";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { registerCommand } from "@/state/commands";
import { LanguageServersPage } from "./LanguageServersPage";
import { startLangserverListeners } from "./store";
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

registerSettingsPage({
  id: "lang.lua",
  title: "Lua",
  group: "Languages & Frameworks",
  order: 30,
  keywords: ["lua", "love", "löve", "neovim", "luajit", "lua-language-server"],
  component: () => <SchemaPage pageId="lang.lua" />,
});

registerCommand({
  id: "langservers.settings",
  title: "Language Servers…",
  category: "Languages & Frameworks",
  run: () => openSettings("lang.servers"),
});

registerSymbolProviders();
startLangserverListeners();
