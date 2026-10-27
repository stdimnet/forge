//! rustup command lines for Settings | Rust: install a toolchain or a component. Forge
//! never runs them on its own; the UI asks and starts them in the Run tool window.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallCommand {
    pub program: String,
    pub args: Vec<String>,
}

fn valid(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
}

/// `rustup toolchain install <toolchain> --profile minimal --component clippy,rustfmt`.
pub fn toolchain(
    rustup: &str,
    toolchain: &str,
    components: &[String],
) -> Result<InstallCommand, String> {
    let t = toolchain.trim();
    if !valid(t) {
        return Err(format!("'{t}' is no toolchain name"));
    }
    let mut args = vec![
        "toolchain".to_string(),
        "install".into(),
        t.to_string(),
        "--profile".into(),
        "minimal".into(),
    ];
    let comps: Vec<&str> = components
        .iter()
        .map(|c| c.trim())
        .filter(|c| valid(c))
        .collect();
    if !comps.is_empty() {
        args.push("--component".into());
        args.push(comps.join(","));
    }
    Ok(InstallCommand {
        program: rustup.to_string(),
        args,
    })
}

/// `rustup component add <component> --toolchain <toolchain>`.
pub fn component(rustup: &str, component: &str, toolchain: &str) -> Result<InstallCommand, String> {
    let c = component.trim();
    if !valid(c) {
        return Err(format!("'{c}' is no component name"));
    }
    let mut args = vec!["component".to_string(), "add".into(), c.to_string()];
    if !toolchain.trim().is_empty() {
        if !valid(toolchain.trim()) {
            return Err(format!("'{}' is no toolchain name", toolchain.trim()));
        }
        args.push("--toolchain".into());
        args.push(toolchain.trim().into());
    }
    Ok(InstallCommand {
        program: rustup.to_string(),
        args,
    })
}

/// `cargo install --locked <crate>` for cargo plugins (cargo-nextest, cargo-llvm-cov).
pub fn cargo_plugin(cargo: &str, krate: &str) -> Result<InstallCommand, String> {
    if !matches!(krate, "cargo-nextest" | "cargo-llvm-cov") {
        return Err(format!("Forge does not install {krate}"));
    }
    Ok(InstallCommand {
        program: cargo.to_string(),
        args: vec!["install".into(), "--locked".into(), krate.into()],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines() {
        let t = toolchain(
            "rustup",
            "nightly-2026-10-20",
            &["clippy".into(), "rust-src".into()],
        )
        .unwrap();
        assert_eq!(
            t.args,
            [
                "toolchain",
                "install",
                "nightly-2026-10-20",
                "--profile",
                "minimal",
                "--component",
                "clippy,rust-src"
            ]
        );
        assert!(toolchain("rustup", "stable; rm -rf /", &[]).is_err());
        let c = component("/home/anna/.cargo/bin/rustup", "rust-analyzer", "stable").unwrap();
        assert_eq!(
            c.args,
            ["component", "add", "rust-analyzer", "--toolchain", "stable"]
        );
        assert_eq!(component("rustup", "rust-src", "").unwrap().args.len(), 3);
        assert!(component("rustup", "", "").is_err());
        assert_eq!(
            cargo_plugin("cargo", "cargo-nextest").unwrap().args,
            ["install", "--locked", "cargo-nextest"]
        );
        assert!(cargo_plugin("cargo", "evil").is_err());
    }
}
