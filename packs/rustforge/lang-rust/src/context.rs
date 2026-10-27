//! The project's Cargo context: what command lines need besides a configuration (the cargo
//! executable, the toolchain, the workspace's members and targets, installed cargo plugins)
//! and the location rules that map libtest's names back to files.

use std::collections::BTreeMap;

use forge_output::LocationRule;
use serde::{Deserialize, Serialize};

use crate::metadata::{module_path, CargoWorkspace, TargetKind};

/// The key under which the backend hands the context to the Cargo Test provider
/// (`CommandContext::settings`).
pub const CONTEXT_KEY: &str = "rust.context";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetInfo {
    pub name: String,
    pub kind: TargetKind,
    pub src_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageInfo {
    pub name: String,
    pub dir: String,
    pub targets: Vec<TargetInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CargoContext {
    /// The cargo executable; empty: `cargo` from the PATH.
    pub cargo: String,
    /// The toolchain for `cargo +<toolchain>` (Settings | Rust); empty: rustup's choice.
    pub toolchain: String,
    /// Whether the active toolchain is a nightly (libtest JSON works without tricks).
    pub nightly: bool,
    /// `auto` (JSON on nightly, plain text otherwise), `json` or `text`.
    pub test_format: String,
    /// `cargo` or `nextest` (when installed).
    pub test_runner: String,
    pub nextest: bool,
    pub llvm_cov: bool,
    /// Environment for cargo (Settings | Rust | Environment).
    pub env: BTreeMap<String, String>,
    /// Features every command gets (Settings | Rust | Cargo).
    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    /// The workspace root, project relative (`""`: the project root).
    pub workspace: String,
    pub packages: Vec<PackageInfo>,
    /// Rust files of the workspace (project relative), for the location rules.
    pub files: Vec<String>,
    pub windows: bool,
}

impl CargoContext {
    /// Members and targets of a workspace.
    pub fn with_workspace(mut self, ws: &CargoWorkspace) -> Self {
        self.workspace = ws.root.clone();
        self.packages = ws
            .members
            .iter()
            .map(|p| PackageInfo {
                name: p.name.clone(),
                dir: p.dir.clone(),
                targets: p
                    .targets
                    .iter()
                    .map(|t| TargetInfo {
                        name: t.name.clone(),
                        kind: t.kind,
                        src_path: t.src_path.clone(),
                    })
                    .collect(),
            })
            .collect();
        self
    }

    pub fn cargo(&self) -> String {
        if self.cargo.is_empty() {
            if self.windows {
                "cargo.exe".into()
            } else {
                "cargo".into()
            }
        } else {
            self.cargo.clone()
        }
    }

    /// `+nightly` and friends, before the subcommand.
    pub fn toolchain_args(&self, own: &str) -> Vec<String> {
        let t = if own.trim().is_empty() {
            self.toolchain.trim()
        } else {
            own.trim()
        };
        if t.is_empty() {
            Vec::new()
        } else {
            vec![format!("+{}", t.trim_start_matches('+'))]
        }
    }

    /// `--manifest-path <workspace>/Cargo.toml` when the workspace is in a sub folder.
    pub fn manifest_args(&self) -> Vec<String> {
        if self.workspace.is_empty() {
            Vec::new()
        } else {
            vec![
                "--manifest-path".into(),
                format!("{}/Cargo.toml", self.workspace),
            ]
        }
    }

    pub fn package(&self, name: &str) -> Option<&PackageInfo> {
        self.packages.iter().find(|p| p.name == name)
    }

    /// The member that contains a project file.
    pub fn package_of(&self, path: &str) -> Option<&PackageInfo> {
        self.packages
            .iter()
            .filter(|p| p.dir.is_empty() || path.starts_with(&format!("{}/", p.dir)))
            .max_by_key(|p| p.dir.len())
    }

    /// The package, target and module path of a source file.
    pub fn crate_of(&self, path: &str) -> Option<(&PackageInfo, &TargetInfo, String)> {
        let pkg = self.package_of(path)?;
        let rank = |k: TargetKind| match k {
            TargetKind::Lib | TargetKind::ProcMacro => 0,
            TargetKind::Bin => 1,
            _ => 2,
        };
        let mut targets: Vec<&TargetInfo> = pkg
            .targets
            .iter()
            .filter(|t| t.kind != TargetKind::BuildScript)
            .collect();
        targets.sort_by_key(|t| (t.src_path != path, rank(t.kind)));
        targets
            .into_iter()
            .find_map(|t| module_path(&t.src_path, path, t.kind).map(|m| (pkg, t, m)))
    }

    /// Rules that turn libtest locations (`shop_core::cart::tests`, doctest and panic paths
    /// relative to the workspace root) into project files: one per module file, the
    /// deepest modules first, crate roots last.
    pub fn location_rules(&self) -> Vec<LocationRule> {
        let mut rules: Vec<(usize, LocationRule)> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        let mut targets: Vec<(&TargetInfo, &PackageInfo)> = self
            .packages
            .iter()
            .flat_map(|p| p.targets.iter().map(move |t| (t, p)))
            .filter(|(t, _)| t.kind != TargetKind::BuildScript)
            .collect();
        // Libraries first: a binary of the same name loses (its crate name collides).
        targets.sort_by_key(|(t, _)| !matches!(t.kind, TargetKind::Lib | TargetKind::ProcMacro));
        for (t, _) in targets {
            let krate = t.name.replace('-', "_");
            let mut files: Vec<(&str, String)> = self
                .files
                .iter()
                .filter_map(|f| module_path(&t.src_path, f, t.kind).map(|m| (f.as_str(), m)))
                .collect();
            if !files.iter().any(|(f, _)| *f == t.src_path) {
                files.push((t.src_path.as_str(), String::new()));
            }
            for (file, module) in files {
                let full = if module.is_empty() {
                    krate.clone()
                } else {
                    format!("{krate}::{module}")
                };
                if seen.contains(&full) {
                    continue;
                }
                seen.push(full.clone());
                let depth = full.matches("::").count();
                rules.push((
                    depth,
                    LocationRule {
                        pattern: format!("^{}(::.*)?$", full),
                        replacement: file.to_string(),
                    },
                ));
            }
        }
        rules.sort_by_key(|r| std::cmp::Reverse(r.0));
        let mut out: Vec<LocationRule> = rules.into_iter().map(|(_, r)| r).collect();
        if !self.workspace.is_empty() {
            // rustc runs in the workspace root: doctest names and panics are relative to it.
            out.push(LocationRule {
                pattern: r"^([^:]+\.rs)$".into(),
                replacement: format!("{}/$1", self.workspace),
            });
        }
        out
    }

    /// Environment of cargo commands. `RUSTUP_AUTO_INSTALL=0` keeps rustup from installing
    /// a missing toolchain or component behind the user's back (Forge asks first).
    pub fn environment(&self) -> BTreeMap<String, String> {
        let mut env = self.env.clone();
        env.entry("RUSTUP_AUTO_INSTALL".into())
            .or_insert_with(|| "0".into());
        env
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::metadata::parse_metadata;

    pub(crate) fn shop_context() -> CargoContext {
        let ws = parse_metadata(
            include_str!("../tests/fixtures/metadata-shop.json"),
            "/home/anna/Projects/shop",
        )
        .unwrap();
        CargoContext {
            files: [
                "crates/shop-core/src/lib.rs",
                "crates/shop-core/src/cart.rs",
                "crates/shop-core/src/money.rs",
                "crates/shop-core/tests/checkout.rs",
                "crates/shop-cli/src/main.rs",
                "crates/shop-cli/examples/report.rs",
            ]
            .map(String::from)
            .to_vec(),
            ..CargoContext::default()
        }
        .with_workspace(&ws)
    }

    fn resolve(rules: &[LocationRule], path: &str) -> Option<String> {
        // A minimal stand-in for the tree's rule matching (first match wins).
        rules.iter().find_map(|r| {
            let pat = r
                .pattern
                .trim_start_matches('^')
                .trim_end_matches("(::.*)?$");
            if r.pattern.ends_with("(::.*)?$") {
                (path == pat || path.starts_with(&format!("{pat}::")))
                    .then(|| r.replacement.clone())
            } else {
                None
            }
        })
    }

    #[test]
    fn rules_map_modules_to_files() {
        let rules = shop_context().location_rules();
        assert_eq!(
            resolve(&rules, "shop_core::cart::tests").as_deref(),
            Some("crates/shop-core/src/cart.rs")
        );
        assert_eq!(
            resolve(&rules, "shop_core::money").as_deref(),
            Some("crates/shop-core/src/money.rs")
        );
        assert_eq!(
            resolve(&rules, "shop_core::tests").as_deref(),
            Some("crates/shop-core/src/lib.rs")
        );
        assert_eq!(
            resolve(&rules, "checkout").as_deref(),
            Some("crates/shop-core/tests/checkout.rs")
        );
        assert_eq!(
            resolve(&rules, "shop").as_deref(),
            Some("crates/shop-cli/src/main.rs")
        );
        assert_eq!(resolve(&rules, "shop_core_x"), None);
        assert!(!rules.iter().any(|r| r.pattern.contains(".rs")));
        let ctx = CargoContext {
            workspace: "rust".into(),
            ..shop_context()
        };
        assert_eq!(ctx.location_rules().last().unwrap().replacement, "rust/$1");
        assert_eq!(ctx.manifest_args(), ["--manifest-path", "rust/Cargo.toml"]);
    }

    #[test]
    fn files_belong_to_crates() {
        let ctx = shop_context();
        let (p, t, m) = ctx.crate_of("crates/shop-core/src/money.rs").unwrap();
        assert_eq!(
            (p.name.as_str(), t.kind, m.as_str()),
            ("shop-core", TargetKind::Lib, "money")
        );
        assert_eq!(ctx.toolchain_args(""), Vec::<String>::new());
        let nightly = CargoContext {
            toolchain: "nightly".into(),
            ..CargoContext::default()
        };
        assert_eq!(nightly.toolchain_args(""), ["+nightly"]);
        assert_eq!(nightly.toolchain_args("+1.85.0"), ["+1.85.0"]);
        assert_eq!(nightly.environment()["RUSTUP_AUTO_INSTALL"], "0");
    }
}
