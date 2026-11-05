#!/usr/bin/env node
/**
 * Validate a Forge plugin with Forge's own rules: runs `forge plugin validate` of the Forge
 * binary (manifest, package limits, icon themes and SVGs, UI bundle, unknown files). Without
 * a Forge binary it falls back to a few basic checks in Node.js and says so. Self-contained
 * (Node.js only), so `forge plugin new` copies it into new plugin projects.
 *
 *   node validate.mjs <folder|file.forgeplugin> [--json] [--strict] [--forge-version <v>]
 *
 * The binary is looked up in `FORGE_BIN`, then in the Cargo target folders of a Forge
 * checkout (`CARGO_TARGET_DIR`, `<repo>/target/{debug,release}`), then on `PATH`. A candidate
 * counts only when `forge plugin --help` answers like Forge (other tools are called `forge`
 * too). Exit codes are the binary's: 0 valid, 1 errors, 2 wrong arguments.
 */
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { delimiter, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const EXE = process.platform === "win32" ? ".exe" : "";

/** Whether `file` is a Forge binary with the plugin commands. */
export function isForge(file) {
  if (!file || !existsSync(file)) return false;
  const r = spawnSync(file, ["plugin", "--help"], { encoding: "utf8", timeout: 30_000 });
  return r.status === 0 && String(r.stdout).includes("forge plugin validate");
}

/** The Forge binary to run, or `null`. `repo` is a Forge checkout to look for builds in. */
export function findForge({ env = process.env, repo } = {}) {
  const candidates = [];
  if (env.FORGE_BIN) candidates.push(env.FORGE_BIN);
  const targets = [env.CARGO_TARGET_DIR, repo && join(repo, "target")].filter(Boolean);
  for (const t of targets) for (const p of ["debug", "release"]) candidates.push(join(t, p, `forge${EXE}`));
  for (const dir of (env.PATH ?? "").split(delimiter).filter(Boolean)) candidates.push(join(dir, `forge${EXE}`));
  return candidates.find(isForge) ?? null;
}

/**
 * The checks that work without Forge: the manifest parses and has its required fields, and
 * the files it names exist. Returns the problems (empty when these pass).
 */
export function basicChecks(dir) {
  const root = resolve(dir);
  const path = join(root, "forge-plugin.json");
  if (!existsSync(path)) return [`${path} is missing`];
  let m;
  try {
    m = JSON.parse(readFileSync(path, "utf8"));
  } catch (e) {
    return [`forge-plugin.json is not valid JSON: ${e instanceof Error ? e.message : e}`];
  }
  const problems = [];
  if (m.manifestVersion !== 1) problems.push("manifestVersion: must be 1");
  if (!/^[a-z0-9-]+(\.[a-z0-9-]+)+$/.test(m.id ?? "") || String(m.id).startsWith("forge.")) {
    problems.push("id: must be a lowercase reverse-DNS id like `publisher.plugin-name`");
  }
  if (typeof m.name !== "string" || !m.name.trim()) problems.push("name: must not be empty");
  if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/.test(m.version ?? "")) {
    problems.push("version: must be a SemVer version like `1.0.0`");
  }
  if (typeof m.engines?.forge !== "string") problems.push("engines.forge: must be a version range like `>=0.1.0`");
  if (m.main && !existsSync(join(root, m.main))) problems.push(`main: ${m.main} does not exist (build the plugin first)`);
  for (const [i, t] of (m.contributes?.iconThemes ?? []).entries()) {
    if (!t.path || !existsSync(join(root, t.path))) problems.push(`contributes.iconThemes[${i}].path: ${t.path} does not exist`);
  }
  return problems;
}

/** Validate `args[0]`; returns the exit code. */
export function validate(args, { repo, log = console.log, error = console.error } = {}) {
  const forge = findForge({ repo });
  if (forge) {
    return spawnSync(forge, ["plugin", "validate", ...args], { stdio: "inherit" }).status ?? 2;
  }
  const target = args.find((a) => !a.startsWith("-"));
  error(
    "Forge binary not found (set FORGE_BIN, put Forge on PATH, or build it with `cargo build -p forge-app`): " +
      "only basic checks ran. `forge plugin validate` checks everything Forge checks on install.",
  );
  if (!target) {
    error("usage: validate <folder|file.forgeplugin> [--json] [--strict] [--forge-version <v>]");
    return 2;
  }
  if (!existsSync(target) || !statSync(target).isDirectory()) {
    error(`${target}: validating a .forgeplugin file needs the Forge binary`);
    return 2;
  }
  const problems = basicChecks(target);
  for (const p of problems) log(`error: ${p}`);
  log(problems.length ? `invalid (${problems.length} basic problem${problems.length === 1 ? "" : "s"})` : "basic checks passed");
  return problems.length ? 1 : 0;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  // In a Forge checkout (sdk/scripts), its builds are found too.
  const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
  process.exitCode = validate(process.argv.slice(2), { repo: existsSync(join(repo, "Cargo.toml")) ? repo : undefined });
}
