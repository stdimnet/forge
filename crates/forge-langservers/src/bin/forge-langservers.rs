//! Pre-install npm language servers the way Forge does (`pnpm langservers` in `app/ui`):
//! Node.js (if missing) and the packages go into Forge's data directory, so `pnpm tauri dev`
//! and installed builds find them at once.
//!
//! ```text
//! forge-langservers install [--accept-intelephense-licence] [package…]   (default: intelephense)
//! forge-langservers updates
//! forge-langservers where
//! ```

use std::sync::Arc;

use forge_langservers::install::InstallOutcome;
use forge_langservers::managed::{updates, Event, Installer, Sources};

const USAGE: &str =
    "usage: forge-langservers install [--accept-intelephense-licence] [package…] | updates | where";

const INTELEPHENSE_NOTICE: &str = "Intelephense is proprietary software (free core, premium features with your own licence key).
Its licence does not allow redistribution, so Forge downloads it from the npm registry onto this machine.
By installing it you agree to the Intelephense licence: https://intelephense.com (LICENSE.txt in the package).
Run again with --accept-intelephense-licence to install it.";

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("install") => install(&args[1..]).await,
        Some("updates") => check_updates().await,
        Some("where") => {
            let d = forge_langservers::Detector::system();
            println!(
                "servers: {}",
                d.managed_dir()
                    .map_or("-".into(), |p| p.display().to_string())
            );
            println!(
                "node.js: {}",
                d.runtimes_dir()
                    .map_or("-".into(), |p| p.display().to_string())
            );
            println!(
                "using:   {}",
                d.node()
                    .map_or("(none yet)".into(), |p| p.display().to_string())
            );
            if let Some((node, script)) = d.managed_launch("intelephense") {
                println!(
                    "intelephense: {} {} --stdio",
                    node.display(),
                    script.display()
                );
            }
            0
        }
        _ => {
            eprintln!("{USAGE}");
            2
        }
    };
    std::process::exit(code);
}

fn installer() -> Result<Installer, String> {
    Installer::system(Sources::new(
        std::env::var("FORGE_NPM_REGISTRY").ok().as_deref(),
        std::env::var("FORGE_NODE_MIRROR").ok().as_deref(),
    ))
}

async fn install(args: &[String]) -> i32 {
    let accepted = args.iter().any(|a| a == "--accept-intelephense-licence");
    let mut packages: Vec<String> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .collect();
    if packages.is_empty() {
        packages.push("intelephense".into());
    }
    if packages
        .iter()
        .any(|p| p.split('@').next() == Some("intelephense"))
        && !accepted
    {
        eprintln!("{INTELEPHENSE_NOTICE}");
        return 1;
    }
    let installer = match installer() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let mut last_percent = None;
    let mut on = |e: Event| match e {
        Event::Step { message } => println!("{message}"),
        Event::Output { line, .. } => println!("  {line}"),
        Event::Progress { label, done, total } => {
            if let Some(t) = total.filter(|t| *t > 0) {
                let p = done * 100 / t;
                if last_percent != Some(p / 10) {
                    last_percent = Some(p / 10);
                    println!("  {label}: {p}%");
                }
            }
        }
    };
    match installer
        .install(&packages, Arc::new(tokio::sync::Notify::new()), &mut on)
        .await
    {
        Ok(InstallOutcome::Succeeded) => 0,
        Ok(other) => {
            eprintln!("install did not finish: {other:?}");
            1
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

async fn check_updates() -> i32 {
    let installer = match installer() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let found = installer
        .check_updates(true, updates::now_secs())
        .await
        .unwrap_or_default();
    if found.is_empty() {
        println!("All servers installed by Forge are up to date.");
    }
    for u in found {
        println!("{} {} -> {}", u.name, u.installed, u.latest);
    }
    0
}
