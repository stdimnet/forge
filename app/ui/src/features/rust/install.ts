/**
 * Installs that Forge only starts with the user's consent: toolchains and components
 * through rustup, cargo plugins through `cargo install --locked`, and rust-analyzer from
 * its GitHub releases (pinned, SHA-256 checked) when rustup cannot provide it. Command
 * lines are built in Rust (`rust_install_command`) and run in the Run tool window.
 */
import { rustApi } from "@/ipc/rust";
import { confirmDialog } from "@/features/browserCompanion/ConfirmDialog";
import { setInputDialog } from "@/features/git/overlays";
import { refreshRustStatus, runRustCommand, rustStatus, showRustBalloon } from "./state";

async function run(kind: "toolchain" | "component" | "cargoPlugin", name: string, toolchain?: string): Promise<boolean> {
  try {
    const cmd = await rustApi.installCommand(kind, name, toolchain);
    const code = await runRustCommand(cmd);
    await refreshRustStatus(true);
    if (code === 0) showRustBalloon(`${name} is installed`, "", 4000);
    return code === 0;
  } catch (e) {
    showRustBalloon(`Cannot install ${name}`, String(e).replace(/^Error: /, ""), 8000);
    return false;
  }
}

/** Install a rustup component (rust-analyzer, rustfmt, clippy, rust-src) for the active toolchain. */
export async function installComponent(component: string): Promise<boolean> {
  const toolchain = rustStatus()?.active?.name ?? "";
  const ok = await confirmDialog({
    title: `Install ${component}`,
    message: `Forge installs the ${component} component of the ${toolchain || "active"} toolchain with rustup.`,
    detail: `rustup component add ${component}${toolchain ? ` --toolchain ${toolchain}` : ""}`,
    question: "Install it now?",
    okLabel: "Install",
  });
  return ok && run("component", component, toolchain);
}

/** Install a cargo plugin (cargo-nextest, cargo-llvm-cov) with cargo install --locked. */
export async function installCargoPlugin(name: "cargo-nextest" | "cargo-llvm-cov"): Promise<boolean> {
  const ok = await confirmDialog({
    title: `Install ${name}`,
    message: `${name} (MIT OR Apache-2.0) is built from crates.io with your toolchain; this takes a few minutes.`,
    detail: `cargo install --locked ${name}`,
    question: "Install it now?",
    okLabel: "Install",
  });
  return ok && run("cargoPlugin", name);
}

/** Ask for a toolchain (stable, beta, nightly, 1.85.0, nightly-2026-10-20) and install it. */
export function installToolchain(): void {
  setInputDialog({
    title: "Install Rust Toolchain",
    label: "Toolchain (rustup toolchain install, with clippy, rustfmt and rust-src):",
    value: "stable",
    okLabel: "Install",
    onOk: (value) => void run("toolchain", value.trim()),
  });
}

/** rust-analyzer: the rustup component when rustup is there, else the GitHub download. */
export async function installAnalyzer(): Promise<boolean> {
  if (rustStatus()?.rustup) return installComponent("rust-analyzer");
  return downloadAnalyzer();
}

export async function downloadAnalyzer(): Promise<boolean> {
  const ok = await confirmDialog({
    title: "Download rust-analyzer",
    message:
      "rust-analyzer (MIT OR Apache-2.0) is not installed. Forge downloads the pinned release for this platform from rust-lang/rust-analyzer on GitHub, checks the SHA-256 digest GitHub publishes for it and keeps it in Forge's data folder.",
    detail: "https://github.com/rust-lang/rust-analyzer/releases",
    question: "Download it now?",
    okLabel: "Download",
  });
  if (!ok) return false;
  showRustBalloon("Downloading rust-analyzer…", "", 4000);
  try {
    const path = await rustApi.downloadAnalyzer();
    await refreshRustStatus(true);
    showRustBalloon("rust-analyzer is ready", path, 5000);
    return true;
  } catch (e) {
    showRustBalloon("Cannot download rust-analyzer", String(e).replace(/^Error: /, ""), 10000);
    return false;
  }
}
