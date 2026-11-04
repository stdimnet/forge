/**
 * Arguments for the Tauri CLI that depend on the build environment (see scripts/tauri.mjs).
 *
 * `tauri build` / `tauri bundle` get one more `--config` (applied last, so it wins):
 * - `bundle.createUpdaterArtifacts`: on when `TAURI_SIGNING_PRIVATE_KEY` is set, so signed
 *   `.sig` files are produced next to the bundles; off otherwise, so builds without the key
 *   (local builds) still succeed. `tauri.agents.conf.json` turns it on for official releases.
 * - `plugins.updater.pubkey`: `FORGE_UPDATER_PUBKEY`, the key the app verifies updates with
 *   (the Rust side also compiles it in with `option_env!`).
 * - `bundle.windows` (Windows only), Authenticode signing of the exe, the NSIS installer and
 *   the MSI (see windowsSigning):
 *   - `FORGE_WINDOWS_CERT_THUMBPRINT`: signtool with a certificate in the user's certificate
 *     store (the release workflow imports the `WINDOWS_CERTIFICATE` pfx and sets it), or
 *   - `FORGE_AZURE_SIGNING_ENDPOINT` + `_ACCOUNT` + `_PROFILE`: Azure Trusted Signing through
 *     `trusted-signing-cli` (which reads `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET` and
 *     `AZURE_TENANT_ID` itself).
 *   Neither set: the bundles are not signed.
 */

/** The default RFC 3161 timestamp server for signtool. */
export const DEFAULT_TIMESTAMP_URL = "http://timestamp.digicert.com";

/**
 * `bundle.windows` for Authenticode signing, or null when the environment has no signing
 * material. A certificate thumbprint wins over Azure Trusted Signing.
 * @param {Record<string, string | undefined>} env
 */
export function windowsSigning(env) {
  const value = (/** @type {string} */ name) => env[name]?.trim() || "";
  const thumbprint = value("FORGE_WINDOWS_CERT_THUMBPRINT").replace(/\s+/g, "");
  if (thumbprint) {
    if (!/^[0-9A-Fa-f]{40}$/.test(thumbprint)) {
      throw new Error("FORGE_WINDOWS_CERT_THUMBPRINT must be the certificate's 40 hex digit SHA-1 thumbprint");
    }
    return {
      certificateThumbprint: thumbprint.toUpperCase(),
      digestAlgorithm: "sha256",
      timestampUrl: value("FORGE_WINDOWS_TIMESTAMP_URL") || DEFAULT_TIMESTAMP_URL,
    };
  }
  const azure = ["ENDPOINT", "ACCOUNT", "PROFILE"].map((k) => value(`FORGE_AZURE_SIGNING_${k}`));
  if (azure.every(Boolean)) {
    const [endpoint, account, profile] = azure;
    return { signCommand: `trusted-signing-cli -e ${endpoint} -a ${account} -c ${profile} -d Forge %1` };
  }
  if (azure.some(Boolean)) {
    throw new Error("Azure Trusted Signing needs FORGE_AZURE_SIGNING_ENDPOINT, _ACCOUNT and _PROFILE");
  }
  return null;
}

/**
 * @param {string[]} args
 * @param {Record<string, string | undefined>} env
 * @param {string} platform `process.platform` of the build machine
 */
export function tauriArgs(args, env, platform = process.platform) {
  const command = args.find((a) => !a.startsWith("-"));
  if (command !== "build" && command !== "bundle") return { args, notes: [] };
  const privateKey = !!env.TAURI_SIGNING_PRIVATE_KEY?.trim();
  const pubkey = env.FORGE_UPDATER_PUBKEY?.trim();
  /** @type {{ bundle: Record<string, unknown>, plugins?: Record<string, unknown> }} */
  const config = { bundle: { createUpdaterArtifacts: privateKey } };
  if (pubkey) config.plugins = { updater: { pubkey } };
  const notes = [];
  if (platform === "win32") {
    const windows = windowsSigning(env);
    if (windows) config.bundle.windows = windows;
    else notes.push("No Windows signing certificate is configured: the bundles are not Authenticode signed.");
  }
  if (!privateKey) notes.push("TAURI_SIGNING_PRIVATE_KEY is not set: no signed updater artifacts are built.");
  if (!pubkey) notes.push("FORGE_UPDATER_PUBKEY is not set: this build will not check for updates.");
  else if (!privateKey) notes.push("FORGE_UPDATER_PUBKEY is set but nothing is signed: publish no update from this build.");
  // Arguments after `--` go to cargo; ours belongs before them.
  const split = args.indexOf("--");
  const extra = ["--config", JSON.stringify(config)];
  const out = split < 0 ? [...args, ...extra] : [...args.slice(0, split), ...extra, ...args.slice(split)];
  return { args: out, notes };
}
