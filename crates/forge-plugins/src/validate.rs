//! Validating a plugin the way Forge installs it, with every problem reported at once (what
//! `forge plugin validate` prints). A folder is packed in memory first ([`crate::pack`]), so
//! exactly the bytes that would ship are checked:
//!
//! - safe zip reading and the package limits ([`crate::package::read_entries`]);
//! - the manifest rules for a Forge version ([`PluginManifest::validate`]);
//! - the UI bundle of code plugins (`main`);
//! - icon themes: the theme format and the SVG rules of `forge_icons`;
//! - files Forge does not use (warnings);
//! - finally [`Package::read`] itself, so a report without errors means Forge installs it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;

use crate::icon_themes::resolve_asset;
use crate::manifest::{is_package_path, BackendSpec, PluginManifest, MANIFEST_FILE};
use crate::pack::{DIST, DOCUMENTS};
use crate::package::Package;

/// Version of the JSON report format (`forge plugin validate --json`).
pub const REPORT_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Error,
    Warning,
}

/// One finding. `code` is one of the stable codes in the CLI reference: `package`,
/// `layout`, `manifest`, `main`, `backend`, `icon-theme`, `svg`, `unknown-file`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    /// JSON path in the manifest or theme file, e.g. `engines.forge`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// Package path of the file the finding is about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

impl Diagnostic {
    fn new(severity: Severity, code: &str, message: impl Into<String>) -> Self {
        Self {
            severity,
            code: code.into(),
            message: message.into(),
            field: None,
            file: None,
        }
    }

    fn error(code: &str, message: impl Into<String>) -> Self {
        Self::new(Severity::Error, code, message)
    }

    fn warning(code: &str, message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, code, message)
    }

    fn field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }

    fn file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageInfo {
    /// Size of the package in bytes.
    pub size: u64,
    /// SHA-256 of the package bytes, lowercase hex (what a marketplace index lists).
    pub sha256: String,
    /// Number of files in the package.
    pub files: usize,
}

/// The result of validating one plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub plugin: Option<PluginInfo>,
    pub package: Option<PackageInfo>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Report {
    pub fn errors(&self) -> usize {
        self.count(Severity::Error)
    }

    pub fn warnings(&self) -> usize {
        self.count(Severity::Warning)
    }

    fn count(&self, severity: Severity) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == severity)
            .count()
    }

    /// No errors, and with `strict` no warnings either.
    pub fn is_valid(&self, strict: bool) -> bool {
        self.errors() == 0 && !(strict && self.warnings() > 0)
    }
}

/// Validate the package bytes of a `.forgeplugin` file for `forge_version`.
pub fn validate_package(bytes: &[u8], forge_version: &str) -> Report {
    let mut report = Report {
        plugin: None,
        package: None,
        diagnostics: Vec::new(),
    };
    let files = match crate::package::read_entries(bytes) {
        Ok(files) => files,
        Err(e) => {
            report.diagnostics.push(Diagnostic::error("package", e));
            return report;
        }
    };
    report.package = Some(PackageInfo {
        size: bytes.len() as u64,
        sha256: crate::sha256_hex(bytes),
        files: files.len(),
    });
    let out = &mut report.diagnostics;
    let Some(manifest) = parse_manifest(&files, out) else {
        return report;
    };
    report.plugin = Some(PluginInfo {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
    });
    for p in manifest.validate(forge_version) {
        out.push(
            Diagnostic::error("manifest", p.message)
                .field(p.field)
                .file(MANIFEST_FILE),
        );
    }
    check_main(&manifest, &files, out);
    let used = check_icon_themes(&manifest, &files, out);
    check_unknown_files(&manifest, &files, &used, out);
    // The installer's own check: nothing above may let through what Forge refuses.
    if report.errors() == 0 {
        if let Err(e) = Package::read(bytes, forge_version) {
            report.diagnostics.push(Diagnostic::error("package", e));
        }
    }
    report
}

/// Validate the plugin folder `dir` (with `forge-plugin.json`): pack it in memory like
/// `forge plugin pack` and validate the result.
pub fn validate_folder(dir: &Path, forge_version: &str) -> Report {
    match crate::pack::pack(dir) {
        Ok(packed) => validate_package(&packed.bytes, forge_version),
        Err(e) => {
            let code = if e.contains(MANIFEST_FILE) {
                "manifest"
            } else if e.contains("64 MB") || e.contains("symbolic links") {
                "package"
            } else {
                "layout"
            };
            let mut report = Report {
                plugin: None,
                package: None,
                diagnostics: vec![Diagnostic::error(code, e)],
            };
            if let Ok(m) = crate::pack::read_manifest(dir) {
                report.plugin = Some(PluginInfo {
                    id: m.id,
                    name: m.name,
                    version: m.version,
                });
            }
            report
        }
    }
}

fn parse_manifest(
    files: &BTreeMap<String, Vec<u8>>,
    out: &mut Vec<Diagnostic>,
) -> Option<PluginManifest> {
    let Some(src) = files.get(MANIFEST_FILE) else {
        out.push(Diagnostic::error(
            "manifest",
            format!("{MANIFEST_FILE} is missing at the package root"),
        ));
        return None;
    };
    let Ok(text) = std::str::from_utf8(src) else {
        out.push(Diagnostic::error("manifest", format!("{MANIFEST_FILE} is not UTF-8")).file(MANIFEST_FILE));
        return None;
    };
    match PluginManifest::parse(text) {
        Ok(m) => Some(m),
        Err(e) => {
            out.push(Diagnostic::error("manifest", e).file(MANIFEST_FILE));
            None
        }
    }
}

fn check_main(
    manifest: &PluginManifest,
    files: &BTreeMap<String, Vec<u8>>,
    out: &mut Vec<Diagnostic>,
) {
    let c = &manifest.contributes;
    match manifest.main.as_deref().filter(|m| is_package_path(m)) {
        Some(main) => match files.get(main) {
            None => out.push(
                Diagnostic::error(
                    "main",
                    format!("`main` names {main}, which is not in the package (build the plugin first)"),
                )
                .field("main")
                .file(main),
            ),
            Some(code) if code.iter().all(u8::is_ascii_whitespace) => out.push(
                Diagnostic::warning("main", "the UI bundle is empty")
                    .field("main")
                    .file(main),
            ),
            Some(code) if std::str::from_utf8(code).is_err() => out.push(
                Diagnostic::error("main", "the UI bundle is not UTF-8 text")
                    .field("main")
                    .file(main),
            ),
            Some(_) => {}
        },
        None if manifest.main.is_none() => {
            let code_parts = [
                ("commands", c.commands.len()),
                ("statusItems", c.status_items.len()),
                ("toolWindows", c.tool_windows.len()),
                ("settingsPages", c.settings_pages.len()),
            ];
            for (field, n) in code_parts {
                if n > 0 {
                    out.push(
                        Diagnostic::warning(
                            "main",
                            "is contributed, but there is no `main` UI bundle to implement it",
                        )
                        .field(format!("contributes.{field}"))
                        .file(MANIFEST_FILE),
                    );
                }
            }
        }
        None => {} // an invalid `main` is a manifest error
    }
    if manifest.backend.is_some() {
        out.push(
            Diagnostic::warning(
                "backend",
                "is part of the format, but this Forge version does not run plugin backends",
            )
            .field("backend")
            .file(MANIFEST_FILE),
        );
    }
}

/// Check every icon theme; returns the package paths the themes use.
fn check_icon_themes(
    manifest: &PluginManifest,
    files: &BTreeMap<String, Vec<u8>>,
    out: &mut Vec<Diagnostic>,
) -> BTreeSet<String> {
    let mut used = BTreeSet::new();
    for (i, theme) in manifest.contributes.icon_themes.iter().enumerate() {
        if !is_package_path(&theme.path) {
            continue; // a manifest error already
        }
        let field = format!("contributes.iconThemes[{i}].path");
        let Some(src) = files.get(&theme.path) else {
            out.push(
                Diagnostic::error("icon-theme", format!("{} is not in the package", theme.path))
                    .field(field)
                    .file(&theme.path),
            );
            continue;
        };
        used.insert(theme.path.clone());
        let def = std::str::from_utf8(src)
            .map_err(|_| "is not UTF-8".to_string())
            .and_then(forge_icons::IconThemeDef::parse);
        let def = match def {
            Ok(d) => d,
            Err(e) => {
                out.push(Diagnostic::error("icon-theme", e).file(&theme.path));
                continue;
            }
        };
        for p in def.validate() {
            let (field, message) = match p.split_once(": ") {
                Some((f, m)) if !f.contains(' ') => (Some(f.to_string()), m.to_string()),
                _ => (None, p.clone()),
            };
            let mut d = Diagnostic::error("icon-theme", message).file(&theme.path);
            d.field = field;
            out.push(d);
        }
        for (id, rel) in &def.icons {
            let field = format!("icons.{id}");
            let Some(path) = resolve_asset(&theme.path, rel) else {
                out.push(
                    Diagnostic::error("svg", format!("`{rel}` leaves the plugin"))
                        .field(field)
                        .file(&theme.path),
                );
                continue;
            };
            if !forge_icons::is_svg_path(&path) {
                continue; // reported by the theme's own validation
            }
            used.insert(path.clone());
            let Some(svg) = files.get(&path) else {
                out.push(
                    Diagnostic::error("svg", format!("{path} is not in the package"))
                        .field(field)
                        .file(&theme.path),
                );
                continue;
            };
            let checked = std::str::from_utf8(svg)
                .map_err(|_| "is not UTF-8".to_string())
                .and_then(forge_icons::check_svg);
            if let Err(e) = checked {
                out.push(Diagnostic::error("svg", e).field(field).file(path));
            }
        }
    }
    used
}

fn check_unknown_files(
    manifest: &PluginManifest,
    files: &BTreeMap<String, Vec<u8>>,
    used: &BTreeSet<String>,
    out: &mut Vec<Diagnostic>,
) {
    let theme_dirs: Vec<String> = manifest
        .contributes
        .icon_themes
        .iter()
        .filter_map(|t| t.path.rsplit_once('/').map(|(d, _)| format!("{d}/")))
        .collect();
    let backend_module = match &manifest.backend {
        Some(BackendSpec::Wasm { module }) => Some(module.as_str()),
        _ => None,
    };
    let dist = format!("{DIST}/");
    for name in files.keys() {
        let known = name == MANIFEST_FILE
            || DOCUMENTS.contains(&name.as_str())
            || manifest.main.as_deref() == Some(name)
            || backend_module == Some(name)
            || used.contains(name)
            || name.starts_with(&dist);
        if known {
            continue;
        }
        let message = if theme_dirs.iter().any(|d| name.starts_with(d)) {
            "is in an icon theme folder, but no icon theme uses it"
        } else {
            "is not used by Forge (not the manifest, a document, `main`, dist/ or an icon theme file)"
        };
        out.push(Diagnostic::warning("unknown-file", message).file(name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icon_themes::tests::{shapes_files, SVG};
    use crate::manifest::tests::WORD_COUNT;
    use crate::package::write;

    fn package(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
        let list: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_slice()))
            .collect();
        write(&list).unwrap()
    }

    fn codes(r: &Report) -> Vec<(Severity, &str)> {
        r.diagnostics
            .iter()
            .map(|d| (d.severity, d.code.as_str()))
            .collect()
    }

    #[test]
    fn a_valid_package_has_no_findings() {
        let bytes = package(&shapes_files());
        let r = validate_package(&bytes, "0.1.0");
        assert_eq!(r.diagnostics, vec![]);
        assert!(r.is_valid(true));
        let info = r.package.unwrap();
        assert_eq!(info.files, 5);
        assert_eq!(info.sha256, crate::sha256_hex(&bytes));
        assert_eq!(r.plugin.unwrap().id, "example.shapes-icons");
    }

    #[test]
    fn reports_every_manifest_problem() {
        let manifest = WORD_COUNT
            .replace("\"1.0.0\"", "\"one\"")
            .replace("\"example.word-count\"", "\"Bad Id\"");
        let bytes = write(&[
            (MANIFEST_FILE, manifest.as_bytes()),
            ("dist/plugin.js", b"x"),
        ])
        .unwrap();
        let r = validate_package(&bytes, "0.1.0");
        let fields: Vec<_> = r
            .diagnostics
            .iter()
            .filter_map(|d| d.field.as_deref())
            .collect();
        assert!(fields.contains(&"id"), "{r:?}");
        assert!(fields.contains(&"version"), "{r:?}");
        assert!(!r.is_valid(false));
    }

    #[test]
    fn reports_icon_theme_and_svg_problems() {
        let mut files = shapes_files();
        files.insert(
            "theme/icons/php.svg".into(),
            b"<svg><script>alert(1)</script></svg>".to_vec(),
        );
        files.remove("theme/icons/folder.svg");
        files.insert("theme/icons/unused.svg".into(), SVG.as_bytes().to_vec());
        files.insert("notes.txt".into(), b"x".to_vec());
        let r = validate_package(&package(&files), "0.1.0");
        assert_eq!(
            codes(&r),
            vec![
                (Severity::Error, "svg"),
                (Severity::Error, "svg"),
                (Severity::Warning, "unknown-file"),
                (Severity::Warning, "unknown-file"),
            ],
            "{r:#?}"
        );
        let script = &r.diagnostics[1];
        assert_eq!(script.file.as_deref(), Some("theme/icons/php.svg"));
        assert_eq!(script.field.as_deref(), Some("icons.php"));
        assert!(script.message.contains("script"), "{script:?}");
        assert!(r.diagnostics[0].message.contains("folder.svg is not in the package"));
    }

    #[test]
    fn code_plugins_need_their_bundle() {
        let no_main = write(&[(MANIFEST_FILE, WORD_COUNT.as_bytes())]).unwrap();
        let r = validate_package(&no_main, "0.1.0");
        assert_eq!(codes(&r), vec![(Severity::Error, "main")]);
        assert!(r.diagnostics[0].message.contains("build the plugin first"));

        let without = r#"{"manifestVersion":1,"id":"acme.x","name":"X","version":"1.0.0",
            "engines":{"forge":">=0.1.0"},
            "contributes":{"commands":[{"id":"acme.x.run","title":"Run"}]}}"#;
        let r = validate_package(&write(&[(MANIFEST_FILE, without.as_bytes())]).unwrap(), "0.1.0");
        assert_eq!(codes(&r), vec![(Severity::Warning, "main")]);
        assert!(r.is_valid(false));
        assert!(!r.is_valid(true));
    }

    #[test]
    fn broken_zips_and_limits_are_package_errors() {
        let r = validate_package(b"nope", "0.1.0");
        assert_eq!(codes(&r), vec![(Severity::Error, "package")]);
        let evil = write(&[(MANIFEST_FILE, WORD_COUNT.as_bytes()), ("../evil.sh", b"x")]).unwrap();
        let r = validate_package(&evil, "0.1.0");
        assert!(r.diagnostics[0].message.contains("unsafe path"), "{r:?}");
    }
}
