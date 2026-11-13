/**
 * Connects Xdebug to run configurations: Debug (Shift+F9) on a PHP configuration starts
 * listening and runs it with the Xdebug environment, and "Stop" falls back to stopping the
 * active run when no debug session is alive.
 */
import { registerDebugLauncher } from "@/features/run";
import { stopSession } from "@/features/run/runStore";
import { debugLauncher } from "./index";
import "./runTypes";

/**
 * Run configuration types executed by the PHP interpreter, and the web ones whose requests
 * Xdebug debugs (PHP Web Page, PHP HTTP Request, requests of `.http` files).
 */
const PHP_TYPES = new Set(["php-script", "phpunit", "artisan", "php-builtin-server", "composer", "php-web-page", "php-http-request", "http-request"]);

registerDebugLauncher({
  id: "xdebug",
  canDebug: (cfg, type) => PHP_TYPES.has(cfg.type) || type?.interpreter === "php",
  async launch(cfg, ctx) {
    const env = await debugLauncher.prepare();
    // The built-in web server debugs every request it serves.
    const interpreterArgs = cfg.type === "php-builtin-server" ? ["-dxdebug.start_with_request=yes"] : undefined;
    await ctx.run({ env, interpreterArgs });
  },
});

debugLauncher.setStopFallback(() => stopSession());
