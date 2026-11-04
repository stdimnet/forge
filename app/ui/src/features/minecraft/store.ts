/** Minecraft plugin state: whether the plugin applies, and running the Paper test server. */
import type { RunConfiguration } from "@/ipc/run";
import { balloonsFor } from "@/keymap";
import { useTemporary } from "@/features/run/configStore";
import { launch } from "@/features/run/runStore";
import { GRADLE } from "@/features/java/runTypes";
import { isPluginActive, usesFramework } from "@/state/project";
import { t } from "@/i18n";

export const PLUGIN_ID = "forge.minecraft";

/** The name of the configuration generated projects get. */
export const RUN_SERVER = "Run Paper Server";

const showBalloon = balloonsFor("Minecraft");

/** The plugin is enabled and the project is a Minecraft plugin. */
export const minecraftActive = () => isPluginActive(PLUGIN_ID) && usesFramework("minecraft");

/** run-paper's `runServer` task as a Gradle configuration (the saved one when it exists). */
export const runServerConfiguration = (): RunConfiguration =>
  useTemporary({ name: RUN_SERVER, type: GRADLE, options: { tasks: "runServer" } });

/** Build the plugin and start a Paper server with it (refused in Safe Mode by the backend). */
export async function runPaperServer(): Promise<void> {
  if (!minecraftActive()) {
    showBalloon(t("langs.minecraft.runPaperServer"), t("langs.minecraft.theProjectIsNotA"));
    return;
  }
  await launch(runServerConfiguration());
}
