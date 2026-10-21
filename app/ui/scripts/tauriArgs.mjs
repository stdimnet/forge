/**
 * Arguments for the Tauri CLI that depend on the build environment (see scripts/tauri.mjs).
 *
 * `tauri build` / `tauri bundle` get one more `--config` (applied last, so it wins):
 * - `bundle.createUpdaterArtifacts`: on when `TAURI_SIGNING_PRIVATE_KEY` is set, so signed
 *   `.sig` files are produced next to the bundles; off otherwise, so builds without the key
 *   (local builds) still succeed. `tauri.agents.conf.json` turns it on for official releases.
 * - `plugins.updater.pubkey`: `FORGE_UPDATER_PUBKEY`, the key the app verifies updates with
 *   (the Rust side also compiles it in with `option_env!`).
 */

/** @param {string[]} args @param {Record<string, string | undefined>} env */
export function tauriArgs(args, env) {
  const command = args.find((a) => !a.startsWith("-"));
  if (command !== "build" && command !== "bundle") return { args, notes: [] };
  const privateKey = !!env.TAURI_SIGNING_PRIVATE_KEY?.trim();
  const pubkey = env.FORGE_UPDATER_PUBKEY?.trim();
  /** @type {Record<string, unknown>} */
  const config = { bundle: { createUpdaterArtifacts: privateKey } };
  if (pubkey) config.plugins = { updater: { pubkey } };
  const notes = [];
  if (!privateKey) notes.push("TAURI_SIGNING_PRIVATE_KEY is not set: no signed updater artifacts are built.");
  if (!pubkey) notes.push("FORGE_UPDATER_PUBKEY is not set: this build will not check for updates.");
  else if (!privateKey) notes.push("FORGE_UPDATER_PUBKEY is set but nothing is signed: publish no update from this build.");
  // Arguments after `--` go to cargo; ours belongs before them.
  const split = args.indexOf("--");
  const extra = ["--config", JSON.stringify(config)];
  const out = split < 0 ? [...args, ...extra] : [...args.slice(0, split), ...extra, ...args.slice(split)];
  return { args: out, notes };
}
