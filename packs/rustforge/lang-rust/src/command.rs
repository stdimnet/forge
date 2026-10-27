//! Cargo command lines: the "Cargo" run configuration (`cargo run`, `test`, `bench`,
//! `build` or any command with the toolchain, package, target, features, profile and
//! target triple of the configuration) and the actions of the Cargo tool window.
//!
//! Configuration options: `command` (`run`, `build --release`, `test --doc`, …), `package`,
//! `target` (`lib`, `bin:shop`, `example:report`, `test:checkout`, `bench:totals`),
//! `features` (comma or space separated), `allFeatures`, `noDefaultFeatures`, `profile`
//! (`dev`, `release` or a custom profile), `targetTriple`, `toolchain`. The program
//! arguments go after `--`.

use std::collections::BTreeMap;

use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use serde::{Deserialize, Serialize};

use crate::context::CargoContext;
use crate::metadata::TargetKind;

/// The run configuration type.
pub const CARGO: &str = "cargo";

/// A command line to start (`cwd` project relative).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CargoCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: BTreeMap<String, String>,
}

/// Which package, target, features and profile a command builds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Selection {
    pub package: String,
    pub target: String,
    pub features: String,
    pub all_features: bool,
    pub no_default_features: bool,
    pub profile: String,
    pub target_triple: String,
}

impl Selection {
    pub fn from_options(cfg: &Configuration) -> Self {
        let opt = |k: &str| cfg.option(k).trim().to_string();
        Selection {
            package: opt("package"),
            target: opt("target"),
            features: opt("features"),
            all_features: opt("allFeatures") == "true",
            no_default_features: opt("noDefaultFeatures") == "true",
            profile: opt("profile"),
            target_triple: opt("targetTriple"),
        }
    }
}

/// `bin:shop` → (`Bin`, `shop`); `lib` → (`Lib`, ``).
pub fn parse_target(key: &str) -> Option<(TargetKind, String)> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    if key == "lib" {
        return Some((TargetKind::Lib, String::new()));
    }
    let (kind, name) = key.split_once(':')?;
    let kind = match kind {
        "bin" => TargetKind::Bin,
        "test" => TargetKind::Test,
        "bench" => TargetKind::Bench,
        "example" => TargetKind::Example,
        _ => return None,
    };
    Some((kind, name.to_string()))
}

/// Subcommands that build a selection of packages and targets.
const BUILDING: &[&str] = &[
    "run", "build", "check", "test", "bench", "clippy", "doc", "rustc", "rustdoc", "nextest",
    "llvm-cov",
];

fn has_any(args: &[String], flags: &[&str]) -> bool {
    args.iter().any(|a| {
        flags
            .iter()
            .any(|f| a == f || a.starts_with(&format!("{f}=")))
    })
}

/// The package, target, feature and profile options for `sub` (only what the subcommand
/// accepts, and nothing the user's own arguments already say).
pub fn selection_args(
    sub: &str,
    sel: &Selection,
    ctx: &CargoContext,
    own: &[String],
) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let builds = BUILDING.contains(&sub);
    if (builds || matches!(sub, "clean" | "update" | "tree"))
        && !sel.package.is_empty()
        && !has_any(own, &["-p", "--package", "--workspace"])
    {
        args.push("-p".into());
        args.push(sel.package.clone());
    }
    if !builds {
        return Ok(args);
    }
    if let Some((kind, name)) = parse_target(&sel.target) {
        if sub == "run" && !matches!(kind, TargetKind::Bin | TargetKind::Example) {
            return Err(format!(
                "cargo run runs binaries and examples, not '{}'",
                sel.target
            ));
        }
        if !has_any(
            own,
            &[
                "--lib",
                "--bin",
                "--bins",
                "--test",
                "--tests",
                "--bench",
                "--benches",
                "--example",
                "--examples",
                "--doc",
                "--all-targets",
            ],
        ) {
            if let Some(flag) = kind.flag() {
                args.push(flag.into());
                if !name.is_empty() {
                    args.push(name);
                }
            }
        }
    }
    let mut features: Vec<String> = ctx.features.clone();
    for f in sel
        .features
        .split([',', ' '])
        .map(str::trim)
        .filter(|f| !f.is_empty())
    {
        if !features.iter().any(|x| x == f) {
            features.push(f.to_string());
        }
    }
    if sel.all_features || ctx.all_features {
        if !has_any(own, &["--all-features"]) {
            args.push("--all-features".into());
        }
    } else if !features.is_empty() && !has_any(own, &["--features", "-F"]) {
        args.push("--features".into());
        args.push(features.join(","));
    }
    if (sel.no_default_features || ctx.no_default_features)
        && !has_any(own, &["--no-default-features"])
    {
        args.push("--no-default-features".into());
    }
    match sel.profile.as_str() {
        "" | "dev" => {}
        _ if has_any(own, &["--release", "-r", "--profile"]) => {}
        "release" => args.push("--release".into()),
        p => {
            // nextest names cargo's profile --cargo-profile; llvm-cov takes --profile.
            args.push(
                if sub == "nextest" {
                    "--cargo-profile"
                } else {
                    "--profile"
                }
                .into(),
            );
            args.push(p.to_string());
        }
    }
    if !sel.target_triple.is_empty() && !has_any(own, &["--target"]) {
        args.push("--target".into());
        args.push(sel.target_triple.clone());
    }
    Ok(args)
}

/// The command line of a "Cargo" run configuration.
pub fn command_line(cfg: &Configuration, ctx: &CargoContext) -> Result<CargoCommand, String> {
    let command = match cfg.option("command").trim() {
        "" => "run".to_string(),
        c => c.trim_start_matches("cargo ").to_string(),
    };
    let own = split_args(&command);
    let Some(sub) = own.first().cloned() else {
        return Err("Enter a cargo command".into());
    };
    let mut args = ctx.toolchain_args(cfg.option("toolchain"));
    args.extend(own.iter().cloned());
    if !has_any(&own, &["--manifest-path"])
        && (BUILDING.contains(&sub.as_str())
            || matches!(sub.as_str(), "clean" | "update" | "tree" | "fmt"))
    {
        args.extend(ctx.manifest_args());
    }
    args.extend(selection_args(
        &sub,
        &Selection::from_options(cfg),
        ctx,
        &own,
    )?);
    let program_args = split_args(&cfg.arguments);
    if !program_args.is_empty() {
        if !own.iter().any(|a| a == "--") {
            args.push("--".into());
        }
        args.extend(program_args);
    }
    Ok(CargoCommand {
        program: ctx.cargo(),
        args,
        cwd: cfg.working_directory.trim().trim_matches('/').to_string(),
        env: ctx.environment(),
    })
}

/// An action of the Cargo tool window: `build`, `check`, `clean`, `update`, `test`,
/// `doc`, `clippy` or `run` (with the selected package and target).
pub fn action_command(
    action: &str,
    sel: &Selection,
    ctx: &CargoContext,
) -> Result<CargoCommand, String> {
    let sub = match action {
        "build" | "check" | "clean" | "update" | "test" | "doc" | "clippy" | "run" | "bench" => {
            action
        }
        other => return Err(format!("Unknown Cargo action '{other}'")),
    };
    let mut args = ctx.toolchain_args("");
    args.push(sub.into());
    args.extend(ctx.manifest_args());
    let mut sel = sel.clone();
    if sub != "run" && sub != "bench" {
        // Build and check work on whole packages; a chosen target narrows run and bench.
        if matches!(sub, "build" | "check" | "clippy" | "doc" | "test") {
            sel.target = match parse_target(&sel.target) {
                Some((TargetKind::Bin | TargetKind::Example | TargetKind::Test, _)) => sel.target,
                _ => String::new(),
            };
        } else {
            sel.target.clear();
        }
    }
    if sel.package.is_empty()
        && matches!(sub, "build" | "check" | "test" | "clippy" | "doc" | "bench")
    {
        args.push("--workspace".into());
    }
    args.extend(selection_args(sub, &sel, ctx, &[])?);
    if sub == "doc" {
        args.push("--no-deps".into());
    }
    Ok(CargoCommand {
        program: ctx.cargo(),
        args,
        cwd: String::new(),
        env: ctx.environment(),
    })
}

/// The temporary "Cargo" configuration that runs a binary or example (`cargo run`).
pub fn run_configuration(package: &str, kind: TargetKind, name: &str) -> Configuration {
    let mut c = Configuration::new(format!("Run {name}"), CARGO);
    c.options.insert("command".into(), "run".into());
    c.options.insert("package".into(), package.into());
    let key = if kind == TargetKind::Example {
        format!("example:{name}")
    } else {
        format!("bin:{name}")
    };
    c.options.insert("target".into(), key);
    c
}

/// A ▶ on `fn main` of a binary or example.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunTarget {
    /// 0-based line of `fn main`.
    pub line: u32,
    /// `shop` (`bin`) or `report` (`example`).
    pub name: String,
    pub kind: TargetKind,
    pub configuration: Configuration,
}

/// The run targets of a file: `fn main` in the crate root of a binary or example.
pub fn run_targets(path: &str, text: &str, ctx: Option<&CargoContext>) -> Vec<RunTarget> {
    let Some(main) = crate::scan::scan(text, "")
        .into_iter()
        .find(|i| i.kind == crate::scan::ItemKind::Main)
    else {
        return Vec::new();
    };
    let path = path.replace('\\', "/");
    let (package, kind, name) = match ctx.and_then(|c| c.crate_of(&path)) {
        Some((p, t, m))
            if m.is_empty() && matches!(t.kind, TargetKind::Bin | TargetKind::Example) =>
        {
            (p.name.clone(), t.kind, t.name.clone())
        }
        Some(_) => return Vec::new(),
        None => {
            let stem = path
                .rsplit('/')
                .next()
                .unwrap_or("")
                .trim_end_matches(".rs")
                .to_string();
            if path.contains("examples/") {
                (String::new(), TargetKind::Example, stem)
            } else if path.contains("src/bin/") {
                (String::new(), TargetKind::Bin, stem)
            } else if path.ends_with("src/main.rs") {
                // The package's default binary.
                let mut c = Configuration::new("Run", CARGO);
                c.options.insert("command".into(), "run".into());
                return vec![RunTarget {
                    line: main.line,
                    name: String::new(),
                    kind: TargetKind::Bin,
                    configuration: c,
                }];
            } else {
                return Vec::new();
            }
        }
    };
    let mut configuration = run_configuration(&package, kind, &name);
    configuration.name = name.clone();
    vec![RunTarget {
        line: main.line,
        name,
        kind,
        configuration,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::tests::shop_context;

    fn cfg(command: &str, options: &[(&str, &str)], arguments: &str) -> Configuration {
        let mut c = Configuration::new("c", CARGO);
        c.options.insert("command".into(), command.into());
        for (k, v) in options {
            c.options.insert((*k).into(), (*v).into());
        }
        c.arguments = arguments.into();
        c
    }

    #[test]
    fn run_configurations() {
        let ctx = shop_context();
        let c = command_line(
            &cfg(
                "run",
                &[
                    ("package", "shop-cli"),
                    ("target", "bin:shop"),
                    ("features", "json, discounts"),
                    ("profile", "release"),
                    ("toolchain", "nightly"),
                ],
                "--customer 'Anna Schmidt'",
            ),
            &ctx,
        )
        .unwrap();
        assert_eq!(
            c.args,
            [
                "+nightly",
                "run",
                "-p",
                "shop-cli",
                "--bin",
                "shop",
                "--features",
                "json,discounts",
                "--release",
                "--",
                "--customer",
                "Anna Schmidt"
            ]
        );
        assert_eq!(c.program, "cargo");
        assert_eq!(c.env["RUSTUP_AUTO_INSTALL"], "0");
        // The user's own options win; custom profiles and target triples.
        let c = command_line(
            &cfg(
                "build --release -p shop-core",
                &[
                    ("package", "shop-cli"),
                    ("profile", "bench-fast"),
                    ("targetTriple", "wasm32-unknown-unknown"),
                    ("noDefaultFeatures", "true"),
                ],
                "",
            ),
            &ctx,
        )
        .unwrap();
        assert_eq!(
            c.args,
            [
                "build",
                "--release",
                "-p",
                "shop-core",
                "--no-default-features",
                "--target",
                "wasm32-unknown-unknown"
            ]
        );
        // Custom commands get the program arguments only.
        let c = command_line(&cfg("fmt --check", &[("package", "x")], ""), &ctx).unwrap();
        assert_eq!(c.args, ["fmt", "--check"]);
        assert!(command_line(&cfg("run", &[("target", "lib")], ""), &ctx).is_err());
        assert!(command_line(&cfg("  ", &[], ""), &ctx).unwrap().args == ["run"]);
    }

    #[test]
    fn tool_window_actions() {
        let ctx = CargoContext {
            workspace: "rust".into(),
            features: vec!["json".into()],
            ..shop_context()
        };
        let all = action_command("check", &Selection::default(), &ctx).unwrap();
        assert_eq!(
            all.args,
            [
                "check",
                "--manifest-path",
                "rust/Cargo.toml",
                "--workspace",
                "--features",
                "json"
            ]
        );
        let sel = Selection {
            package: "shop-cli".into(),
            target: "example:report".into(),
            ..Selection::default()
        };
        let run = action_command("run", &sel, &ctx).unwrap();
        assert_eq!(
            run.args,
            [
                "run",
                "--manifest-path",
                "rust/Cargo.toml",
                "-p",
                "shop-cli",
                "--example",
                "report",
                "--features",
                "json"
            ]
        );
        let clean = action_command("clean", &sel, &ctx).unwrap();
        assert_eq!(
            clean.args,
            [
                "clean",
                "--manifest-path",
                "rust/Cargo.toml",
                "-p",
                "shop-cli"
            ]
        );
        assert_eq!(
            action_command("update", &Selection::default(), &ctx)
                .unwrap()
                .args,
            ["update", "--manifest-path", "rust/Cargo.toml"]
        );
        assert!(action_command("publish", &sel, &ctx).is_err());
        let r = run_configuration("shop-cli", TargetKind::Bin, "shop");
        assert_eq!(r.option("target"), "bin:shop");
    }

    #[test]
    fn main_functions_of_binaries_and_examples() {
        let ctx = shop_context();
        let main = include_str!("../tests/fixtures/shop/crates/shop-cli/src/main.rs");
        let t = run_targets("crates/shop-cli/src/main.rs", main, Some(&ctx));
        assert_eq!(t.len(), 1);
        assert_eq!(
            (t[0].line, t[0].name.as_str(), t[0].kind),
            (4, "shop", TargetKind::Bin)
        );
        assert_eq!(t[0].configuration.option("package"), "shop-cli");
        let example = include_str!("../tests/fixtures/shop/crates/shop-cli/examples/report.rs");
        let t = run_targets("crates/shop-cli/examples/report.rs", example, Some(&ctx));
        assert_eq!(t[0].configuration.option("target"), "example:report");
        // A `fn main` in a library module is no entry point.
        assert!(
            run_targets("crates/shop-core/src/money.rs", "fn main() {}", Some(&ctx)).is_empty()
        );
        assert_eq!(
            run_targets("src/bin/tool.rs", "fn main() {}", None)[0]
                .configuration
                .option("target"),
            "bin:tool"
        );
    }
}
