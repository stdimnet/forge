/**
 * Browser Companion: open files from the browser extension, forge:// links and the `forge`
 * launcher, plus its settings page (Settings | Tools | Browser Companion).
 */
import { registerSettingsPage } from "@/features/settingsPages";
import { openSettings } from "@/features/settings/dialogState";
import { registerCommand } from "@/state/commands";
import { BrowserCompanionPage } from "./BrowserCompanionPage";
import { listenForOpenRequests } from "./openRequests";
import "./companion.css";

registerSettingsPage({
  id: "tools.browser-companion",
  title: "Browser Companion",
  group: "Tools",
  order: 20,
  keywords: [
    "browser",
    "extension",
    "Chrome",
    "Firefox",
    "pair",
    "React",
    "Vue",
    "Angular",
    "Svelte",
    "forge://",
    "Ignition",
    "Symfony",
    "Xdebug",
    "Whoops",
    "launch-editor",
    "IGNITION_EDITOR",
    "built-in server",
    "port",
  ],
  component: BrowserCompanionPage,
});

registerCommand({
  id: "BrowserCompanion.Settings",
  title: "Browser Companion Settings",
  category: "Tools",
  run: () => openSettings("tools.browser-companion"),
});

void listenForOpenRequests().catch((e) => console.warn("Browser Companion is unavailable:", e));
