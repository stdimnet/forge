/**
 * WebForge pack: HTML, CSS & JavaScript (forge.web: npm tool window, package.json run icons,
 * package managers, Node.js and TypeScript settings with the web language servers), React
 * (forge.react), Vue.js (forge.vue) and Angular (forge.angular) navigation and line markers.
 * Each part is active only when the project uses it.
 */
import { Hexagon } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { LanguageServersPage } from "@/features/langservers/LanguageServersPage";
import { registerCommand } from "@/state/commands";
import { registerKeymapActions } from "@/keymap/store";
import { toggleToolWindow } from "@/state/layout";
import { NpmActions, NpmView } from "./NpmView";
import { loadPackages, npmActive, packages } from "./store";
import { installDependencies } from "./npm";
import "./editor";
import "./jsDebug";
import "./popup";
import "./webforge.css";
import { t } from "@/i18n";

registerToolWindow({
  id: "npm",
  title: t("toolwindow.npm.title"),
  icon: Hexagon,
  anchor: "left",
  order: 30,
  component: NpmView,
  actions: NpmActions,
  when: npmActive,
});

registerSettingsPage({
  id: "lang.node",
  title: t("settings.page.lang.node"),
  group: "Languages & Frameworks",
  order: 20,
  keywords: ["node", "npm", "pnpm", "yarn", "bun", "package manager", "scripts", "package.json", "inspect", "debug"],
  component: () => <SchemaPage pageId="lang.node" />,
});

/** Language servers of the web languages, shown on the TypeScript page like WebStorm's services. */
const WEB_SERVER_GROUPS = ["JavaScript & TypeScript", "Vue", "Angular", "HTML & CSS"];

registerSettingsPage({
  id: "lang.typescript",
  title: t("settings.page.lang.typescript"),
  group: "Languages & Frameworks",
  order: 21,
  keywords: [
    "typescript",
    "javascript",
    "react",
    "vue",
    "volar",
    "angular",
    "ngserver",
    "tsserver",
    "typescript-language-server",
    "language service",
    "components",
  ],
  component: () => (
    <div class="wf-ts-page">
      <SchemaPage pageId="lang.typescript" />
      <h3 class="set-section-title wf-ts-servers-title">{t("langs.webforge.languageServices")}</h3>
      <LanguageServersPage groups={WEB_SERVER_GROUPS} />
    </div>
  ),
});

registerKeymapActions([
  { id: "ActivatenpmToolWindow", title: "npm", group: "Tool Windows" },
  { id: "npm.reload", title: "Reload npm Scripts", group: "Plugins/npm" },
  { id: "npm.install", title: "Install Dependencies", group: "Plugins/npm" },
]);
registerCommand({ id: "ActivatenpmToolWindow", title: "npm", category: "View", run: () => toggleToolWindow("npm") });
registerCommand({ id: "npm.reload", title: "Reload npm Scripts", category: "Tools", run: () => void loadPackages(true) });
registerCommand({
  id: "npm.install",
  title: "Install Dependencies",
  category: "Tools",
  run: () => {
    const root = packages()[0];
    if (root) void installDependencies(root);
  },
});
