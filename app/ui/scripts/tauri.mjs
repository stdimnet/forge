/**
 * `pnpm tauri …`: runs the Tauri CLI from `app/` and adds the updater settings of the build
 * environment and, on Windows, the signing settings and the MSI version to `tauri build` (see
 * tauriArgs.mjs). The release workflow calls it (tauri-action's `tauriScript`) as
 * `pnpm tauri build --config src-tauri/tauri.agents.conf.json`.
 */
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { tauriArgs } from "./tauriArgs.mjs";

const app = resolve(import.meta.dirname, "../..");
const cli = createRequire(import.meta.url).resolve("@tauri-apps/cli/tauri.js");
const { version } = JSON.parse(readFileSync(resolve(app, "src-tauri/tauri.conf.json"), "utf8"));
const { args, notes } = tauriArgs(process.argv.slice(2), process.env, process.platform, version);
for (const note of notes) console.log(`[forge] ${note}`);
const result = spawnSync(process.execPath, [cli, ...args], {
  cwd: app,
  stdio: "inherit",
});
process.exit(result.status ?? 1);
