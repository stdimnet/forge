/**
 * Connects Xdebug to run configurations: Debug (Shift+F9) on a PHP configuration starts
 * listening and runs it with the Xdebug environment, and "Stop" falls back to stopping the
 * active run when no debug session is alive.
 */
import { registerDebugLauncher } from "@/features/run";
import { stopSession } from "@/features/run/runStore";
import { debugLauncher } from "./index";

/** Run configuration types executed by the PHP interpreter. */
const PHP_TYPES = new Set(["php-script", "phpunit", "artisan", "php-builtin-server", "composer"]);

registerDebugLauncher({
  id: "xdebug",
  canDebug: (cfg, type) => PHP_TYPES.has(cfg.type) || type?.interpreter === "php",
  async launch(_cfg, ctx) {
    const env = await debugLauncher.prepare();
    await ctx.run({ env });
  },
});

debugLauncher.setStopFallback(() => stopSession());
