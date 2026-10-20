/**
 * Builds the Forge agent for remote hosts: statically linked (musl) Linux binaries for
 * x86_64 and aarch64, written to target/agents/forge-remote-agent-<arch>-linux. Forge looks
 * for them in `FORGE_REMOTE_AGENT_DIR`, the bundle's `agents/` resource folder and next to
 * its executable, and uploads the matching one to `~/.cache/forge-agent/<version>/` on the
 * host.
 *
 * Needs `rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl`. The
 * tree-sitter C code is compiled with the host's gcc for the native architecture (fortify
 * off, since musl has no `__*_chk` functions) unless `CC_<triple>` is set; aarch64 needs a
 * cross compiler and linker (`CC_aarch64_unknown_linux_musl`,
 * `CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER`). Targets that are not installed are
 * skipped with a hint. Usage: `pnpm agents [x86_64|aarch64 ...]`.
 */
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "../../..");
const target = process.env.CARGO_TARGET_DIR ?? resolve(root, "target");
const outDir = resolve(target, "agents");
const all = { x86_64: "x86_64-unknown-linux-musl", aarch64: "aarch64-unknown-linux-musl" };
const wanted = process.argv.slice(2).length ? process.argv.slice(2) : Object.keys(all);

const installed = execFileSync("rustup", ["target", "list", "--installed"], { encoding: "utf8" }).split("\n");
mkdirSync(outDir, { recursive: true });
for (const arch of wanted) {
  const triple = all[arch];
  if (!triple) throw new Error(`unknown architecture ${arch} (x86_64 or aarch64)`);
  if (!installed.includes(triple)) {
    console.warn(`skipping ${arch}: run \`rustup target add ${triple}\` first`);
    continue;
  }
  const env = { ...process.env };
  const key = triple.replaceAll("-", "_");
  if (!env[`CC_${key}`] && process.arch === (arch === "x86_64" ? "x64" : "arm64")) {
    env[`CC_${key}`] = "gcc";
    env[`CFLAGS_${key}`] ??= "-U_FORTIFY_SOURCE -D_FORTIFY_SOURCE=0";
  }
  execFileSync("cargo", ["build", "--release", "-p", "forge-remote-agent", "--target", triple], {
    cwd: root,
    stdio: "inherit",
    env,
  });
  const out = resolve(outDir, `forge-remote-agent-${arch}-linux`);
  copyFileSync(resolve(target, triple, "release/forge-remote-agent"), out);
  console.log(`wrote ${out}`);
}
