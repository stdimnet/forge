/**
 * Tools | WP-CLI…: the Command Line Tools Console with `wp ` typed in, so commands, options
 * and arguments complete from `wp cli cmd-dump` (cached in the user cache dir by the
 * Command Line Tool Support). Runs go through `cli_tools_run`, which refuses in Safe Mode.
 */
import { openToolConsole } from "@/features/cliTools/openTool";
import { wordpressInfo } from "./store";

export function runWpCli(): void {
  const where = wordpressInfo()?.wpCli;
  void openToolConsole(
    "wp-cli",
    "WP-CLI",
    where
      ? `Add WP-CLI (${where.path}) in Settings | Tools | Command Line Tool Support.`
      : "WP-CLI was not found in vendor/bin, as wp-cli.phar or on PATH. Install it (composer require --dev wp-cli/wp-cli-bundle) or add it in Settings | Tools | Command Line Tool Support.",
  );
}
