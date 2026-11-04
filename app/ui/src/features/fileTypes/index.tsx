/**
 * Settings | Editor | File Types: patterns of the recognized file types, custom file types
 * with their own highlighting, and the ignored files and folders. Detection with these
 * settings happens in Rust (`forge_filetypes::user`); custom types are highlighted by
 * `editor/customFileType.ts`.
 */
import { createEffect, createRoot, on } from "solid-js";
import { registerSettingsPage } from "@/features/settingsPages";
import { getSetting } from "@/state/settings";
import { refreshAllDirs } from "@/features/project/ProjectView";
import { FileTypesPage, IGNORED_KEY } from "./FileTypesPage";
import "./override";
import "./fileTypes.css";
import { t } from "@/i18n";

registerSettingsPage({
  id: "editor.file-types",
  title: t("settings.page.editor.file-types"),
  group: "Editor",
  order: 20,
  keywords: ["file name patterns", "wildcard", "associate", "custom file type", "ignored files and folders", "keywords", "syntax highlighting"],
  component: FileTypesPage,
});

// The Project view follows the ignored files and folders.
createRoot(() => createEffect(on(() => getSetting<string>(IGNORED_KEY, ""), () => refreshAllDirs(), { defer: true })));
