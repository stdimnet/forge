import { registerSettingsPage } from "@/features/settingsPages";
import { KeymapSettingsPage } from "./KeymapSettingsPage";
import { t } from "@/i18n";

registerSettingsPage({
  id: "keymap",
  title: t("settings.page.keymap"),
  group: "Keymap",
  order: 20,
  keywords: ["shortcut", "hotkey", "keyboard", "binding", "phpstorm", "macos"],
  component: KeymapSettingsPage,
});
