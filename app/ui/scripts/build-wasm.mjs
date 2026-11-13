/**
 * Builds crates/forge-wasm to WebAssembly and generates the JS bindings in src/wasm/pkg.
 * Requires the `wasm32-unknown-unknown` target and `wasm-bindgen-cli` matching the
 * `wasm-bindgen` version in crates/forge-wasm/Cargo.toml.
 */
import { execFileSync } from "node:child_process";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "../../..");
const target = process.env.CARGO_TARGET_DIR ?? resolve(root, "target");
const run = (cmd, args) => execFileSync(cmd, args, { cwd: root, stdio: "inherit" });

run("cargo", ["build", "-p", "forge-wasm", "--profile", "wasm", "--target", "wasm32-unknown-unknown"]);
run("wasm-bindgen", [
  resolve(target, "wasm32-unknown-unknown/wasm/forge_wasm.wasm"),
  "--target", "web",
  "--out-dir", resolve(root, "app/ui/src/wasm/pkg"),
]);
console.log("wrote app/ui/src/wasm/pkg");
