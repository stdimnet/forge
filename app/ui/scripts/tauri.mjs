/**
 * `pnpm tauri …`: runs the Tauri CLI from `app/` and adds the updater settings of the build
 * environment to `tauri build` (see tauriArgs.mjs). The release workflow calls it as
 * `pnpm tauri build --config src-tauri/tauri.agents.conf.json`.
 */
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { tauriArgs } from "./tauriArgs.mjs";

const cli = createRequire(import.meta.url).resolve("@tauri-apps/cli/tauri.js");
const { args, notes } = tauriArgs(process.argv.slice(2), process.env);
for (const note of notes) console.log(`[forge] ${note}`);
const result = spawnSync(process.execPath, [cli, ...args], {
  cwd: resolve(import.meta.dirname, "../.."),
  stdio: "inherit",
});
process.exit(result.status ?? 1);
