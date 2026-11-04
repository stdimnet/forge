/**
 * Minecraft plugins (`forge.minecraft`, JavaForge), like IntelliJ's Minecraft Development
 * plugin. Everything is computed in Rust (`packs/javaforge/minecraft`,
 * `commands/minecraft.rs`); this module plugs it into the contribution points:
 *
 * - `plugin.yml`, `paper-plugin.yml`, `bungee.yml`, `velocity-plugin.json`: the `main` class
 *   is checked and Ctrl+B opens it (completion and validation of the other keys come from
 *   the bundled JSON schemas, `features/jsonSchema`);
 * - gutter icons: event handlers (`@EventHandler`, `@Subscribe`) and ▶ Run Paper Server on
 *   the plugin's main class;
 * - the command "Run Paper Server" (run-paper's `runServer` Gradle task; refused in Safe Mode
 *   like every run of project code);
 * - Settings | Languages & Frameworks | Java | Minecraft.
 *
 * Active only for projects whose build depends on a Minecraft server or proxy API. The New
 * Project wizard's Minecraft Plugin generator lives in `forge-project`.
 */
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { registerCommand } from "@/state/commands";
import { runPaperServer } from "./store";
import "./editor";
import "./minecraft.css";
import { t } from "@/i18n";

registerSettingsPage({
  id: "lang.java.minecraft",
  title: t("settings.page.lang.java.minecraft"),
  group: "Languages & Frameworks",
  parent: "lang.java",
  order: 20,
  keywords: ["minecraft", "paper", "spigot", "bukkit", "purpur", "folia", "velocity", "bungeecord", "plugin.yml", "event handler", "run-paper"],
  component: () => <SchemaPage pageId="lang.java.minecraft" />,
});

registerCommand({ id: "minecraft.runServer", title: "Run Paper Server", category: "Minecraft", run: () => void runPaperServer() });
