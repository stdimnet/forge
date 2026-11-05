//! `forge plugin …`, the plugin SDK commands of the Forge binary (headless, no window):
//!
//! ```text
//! forge plugin validate <folder|file.forgeplugin> [--json] [--strict] [--forge-version <v>]
//! forge plugin pack <folder> [-o <file.forgeplugin>] [--json] [--forge-version <v>]
//! ```
//!
//! Exit codes: 0 valid (or packed), 1 the plugin has errors (or warnings with `--strict`),
//! 2 wrong arguments or a path that is neither a plugin folder nor a package file.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::validate::{self, Diagnostic, PackageInfo, PluginInfo, Report, Severity};

pub const USAGE: &str = "Usage:
  forge plugin validate <folder|file.forgeplugin> [--json] [--strict] [--forge-version <version>]
  forge plugin pack <folder> [-o <file.forgeplugin>] [--json] [--forge-version <version>]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Validate,
    Pack { out: Option<PathBuf> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub command: Command,
    pub path: PathBuf,
    pub json: bool,
    pub strict: bool,
    pub forge_version: String,
}

/// Parse the arguments after `plugin`. `Ok(None)` asks for the usage text (`--help`).
pub fn parse(args: &[String]) -> Result<Option<Args>, String> {
    let Some(command) = args.first() else {
        return Err(USAGE.into());
    };
    if matches!(command.as_str(), "-h" | "--help" | "help") {
        return Ok(None);
    }
    let pack = match command.as_str() {
        "validate" => false,
        "pack" => true,
        other => return Err(format!("unknown command 'plugin {other}'\n{USAGE}")),
    };
    let mut positional = Vec::new();
    let mut json = false;
    let mut strict = false;
    let mut out = None;
    let mut forge_version = crate::FORGE_VERSION.to_string();
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = true,
            "--strict" if !pack => strict = true,
            "-o" | "--out" if pack => {
                out = Some(PathBuf::from(it.next().ok_or("-o needs a file")?));
            }
            "--forge-version" => {
                let v = it.next().ok_or("--forge-version needs a version")?;
                semver::Version::parse(v)
                    .map_err(|e| format!("--forge-version {v} is not a SemVer version: {e}"))?;
                forge_version = v.clone();
            }
            "-h" | "--help" => return Ok(None),
            _ if a.starts_with('-') && a.len() > 1 => {
                return Err(format!("unknown option '{a}'\n{USAGE}"))
            }
            _ => positional.push(a.clone()),
        }
    }
    let [path] = positional.as_slice() else {
        return Err(USAGE.into());
    };
    Ok(Some(Args {
        command: if pack {
            Command::Pack { out }
        } else {
            Command::Validate
        },
        path: PathBuf::from(path),
        json,
        strict,
        forge_version,
    }))
}

/// The JSON report (`--json`), schema [`validate::REPORT_SCHEMA`].
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonReport<'a> {
    pub schema: u32,
    pub forge_version: &'a str,
    pub path: String,
    /// `folder` or `package`.
    pub kind: &'a str,
    pub strict: bool,
    pub valid: bool,
    pub errors: usize,
    pub warnings: usize,
    pub plugin: Option<&'a PluginInfo>,
    pub package: Option<&'a PackageInfo>,
    /// `forge plugin pack`: the written package file (absent when nothing was written).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    pub diagnostics: &'a [Diagnostic],
}

/// Run `forge plugin <args>`; returns the exit code.
pub fn run(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let args = match parse(args) {
        Ok(Some(a)) => a,
        Ok(None) => {
            let _ = writeln!(stdout, "{USAGE}");
            return 0;
        }
        Err(e) => {
            let _ = writeln!(stderr, "{e}");
            return 2;
        }
    };
    let is_dir = args.path.is_dir();
    let kind = if is_dir { "folder" } else { "package" };
    let report = match &args.command {
        Command::Pack { .. } if !is_dir => {
            let _ = writeln!(
                stderr,
                "forge plugin pack: {} is not a plugin folder",
                args.path.display()
            );
            return 2;
        }
        _ if is_dir => {
            if !args.path.join(crate::manifest::MANIFEST_FILE).is_file() {
                let _ = writeln!(
                    stderr,
                    "forge plugin: {} has no {}",
                    args.path.display(),
                    crate::manifest::MANIFEST_FILE
                );
                return 2;
            }
            validate::validate_folder(&args.path, &args.forge_version)
        }
        _ => match std::fs::read(&args.path) {
            Ok(bytes) if args.path.is_file() => {
                validate::validate_package(&bytes, &args.forge_version)
            }
            _ => {
                let _ = writeln!(
                    stderr,
                    "forge plugin: {} is neither a plugin folder nor a .{} file",
                    args.path.display(),
                    crate::package::EXTENSION
                );
                return 2;
            }
        },
    };
    let mut valid = report.is_valid(args.strict);
    let mut output = None;
    if let (Command::Pack { out }, true) = (&args.command, valid) {
        match write_package(&args.path, out.as_deref()) {
            Ok(file) => output = Some(file),
            Err(e) => {
                let _ = writeln!(stderr, "forge plugin pack: {e}");
                valid = false;
            }
        }
    }
    if args.json {
        let json = JsonReport {
            schema: validate::REPORT_SCHEMA,
            forge_version: &args.forge_version,
            path: args.path.display().to_string(),
            kind,
            strict: args.strict,
            valid,
            errors: report.errors(),
            warnings: report.warnings(),
            plugin: report.plugin.as_ref(),
            package: report.package.as_ref(),
            output: output.as_ref().map(|p| p.display().to_string()),
            diagnostics: &report.diagnostics,
        };
        let _ = writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&json).unwrap_or_default()
        );
    } else {
        print_human(&args, kind, &report, valid, output.as_deref(), stdout);
    }
    i32::from(!valid)
}

/// Pack again and write the package; the bytes are the ones that were validated, because
/// packing is reproducible.
fn write_package(dir: &Path, out: Option<&Path>) -> Result<PathBuf, String> {
    let packed = crate::pack::pack(dir)?;
    let file = match out {
        Some(f) => f.to_path_buf(),
        None => dir.join(packed.file_name()),
    };
    std::fs::write(&file, &packed.bytes)
        .map_err(|e| format!("cannot write {}: {e}", file.display()))?;
    Ok(file)
}

fn print_human(
    args: &Args,
    kind: &str,
    report: &Report,
    valid: bool,
    output: Option<&Path>,
    out: &mut dyn Write,
) {
    let name = match &report.plugin {
        Some(p) => format!("{} {}", p.id, p.version),
        None => args.path.display().to_string(),
    };
    let _ = match &report.package {
        Some(p) => writeln!(
            out,
            "{name} ({kind}, {} file{}, {} bytes) for Forge {}",
            p.files,
            if p.files == 1 { "" } else { "s" },
            p.size,
            args.forge_version
        ),
        None => writeln!(out, "{name} ({kind}) for Forge {}", args.forge_version),
    };
    for d in &report.diagnostics {
        let level = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let at = match (&d.file, &d.field) {
            (Some(file), Some(field)) => format!(" {file} {field}:"),
            (Some(file), None) => format!(" {file}:"),
            (None, Some(field)) => format!(" {field}:"),
            (None, None) => String::new(),
        };
        let _ = writeln!(out, "{level}[{}]{at} {}", d.code, d.message);
    }
    let (errors, warnings) = (report.errors(), report.warnings());
    let plural = |n: usize, w: &str| format!("{n} {w}{}", if n == 1 { "" } else { "s" });
    let summary = format!(
        "{}, {}",
        plural(errors, "error"),
        plural(warnings, "warning")
    );
    let _ = if valid {
        writeln!(out, "valid ({summary})")
    } else if errors == 0 && args.strict {
        writeln!(out, "invalid with --strict ({summary})")
    } else {
        writeln!(out, "invalid ({summary})")
    };
    if let (Some(file), Some(p)) = (output, &report.package) {
        let _ = writeln!(
            out,
            "wrote {} ({} bytes)\nsha256 {}",
            file.display(),
            p.size,
            p.sha256
        );
    }
}

/// `forge plugin …` from the process arguments (`argv[1] == "plugin"`): `None` otherwise.
pub fn main(argv: &[String]) -> Option<i32> {
    if argv.get(1).map(String::as_str) != Some("plugin") {
        return None;
    }
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    Some(run(&argv[2..], &mut stdout, &mut stderr))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn parses_arguments() {
        let a = parse(&s(&[
            "validate",
            "p",
            "--json",
            "--strict",
            "--forge-version",
            "9.0.0",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(a.command, Command::Validate);
        assert!(a.json && a.strict);
        assert_eq!(a.forge_version, "9.0.0");
        let a = parse(&s(&["pack", "p", "-o", "x.forgeplugin"]))
            .unwrap()
            .unwrap();
        assert_eq!(
            a.command,
            Command::Pack {
                out: Some("x.forgeplugin".into())
            }
        );
        assert_eq!(a.forge_version, crate::FORGE_VERSION);
        assert_eq!(parse(&s(&["--help"])), Ok(None));
        for bad in [
            &[][..],
            &["validate"][..],
            &["validate", "a", "b"],
            &["validate", "a", "-o", "x"],
            &["pack", "a", "--strict"],
            &["publish", "a"],
            &["validate", "a", "--forge-version", "latest"],
        ] {
            assert!(parse(&s(bad)).is_err(), "{bad:?}");
        }
        assert_eq!(main(&s(&["forge", "inspect"])), None);
    }
}
