import { registerSettingsPage } from "@/features/settingsPages";
import { KeymapSettingsPage } from "./KeymapSettingsPage";

registerSettingsPage({
  id: "keymap",
  title: "Keymap",
  group: "Keymap",
  order: 20,
  keywords: ["shortcut", "hotkey", "keyboard", "binding", "phpstorm", "macos"],
  component: KeymapSettingsPage,
});
