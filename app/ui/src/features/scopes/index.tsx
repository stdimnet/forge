/**
 * Scopes and file colors: the Scopes and File Colors settings pages (Appearance & Behavior)
 * and the scope chooser used by Find in Files, Find Usages and Structural Search. File
 * colors tint editor tabs and Project view rows (`tabColor`, `projectViewColor`).
 */
import { registerSettingsPage } from "@/features/settingsPages";
import { ScopesPage } from "./ScopesPage";
import { FileColorsPage } from "./FileColorsPage";
import { SCOPES_PAGE } from "./ScopeSelect";
import "./scopes.css";
import { t } from "@/i18n";

registerSettingsPage({
  id: SCOPES_PAGE,
  title: t("settings.page.scopes"),
  group: "Appearance & Behavior",
  order: 20,
  keywords: ["scope", "pattern", "file:", "include", "exclude", "shared", "local"],
  component: ScopesPage,
});

registerSettingsPage({
  id: "appearance.fileColors",
  title: t("settings.page.appearance.fileColors"),
  group: "Appearance & Behavior",
  order: 21,
  keywords: ["file colors", "tabs", "project view", "tests", "non-project files", "tint"],
  component: FileColorsPage,
});
