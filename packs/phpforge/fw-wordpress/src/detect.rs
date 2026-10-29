//! Is the project WordPress, and what kind: a full site (`wp-config.php`, `wp-includes`), a
//! Bedrock / Composer install (`johnpbloch/wordpress`, `roots/*`), a `wp-content` folder, or
//! a single plugin or theme recognised by its file header (`Plugin Name:` in a PHP file,
//! `Theme Name:` in `style.css`). Also finds WP-CLI and the WordPress stubs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Read access to the project's files, relative paths with `/` separators.
pub trait Probe {
    fn exists(&self, rel: &str) -> bool;
    fn read(&self, rel: &str) -> Option<String>;
    /// File names directly in `dir` ("" for the root).
    fn files_in(&self, dir: &str) -> Vec<String>;
}

/// A project folder on disk.
pub struct DirProbe<'a>(pub &'a Path);

impl Probe for DirProbe<'_> {
    fn exists(&self, rel: &str) -> bool {
        self.0.join(rel).exists()
    }
    fn read(&self, rel: &str) -> Option<String> {
        let path = self.0.join(rel);
        // Headers are at the top: read at most 64 KiB.
        let bytes = std::fs::read(path).ok()?;
        let end = bytes.len().min(64 * 1024);
        Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }
    fn files_in(&self, dir: &str) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(self.0.join(dir))
            .map(|it| {
                it.flatten()
                    .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        out
    }
}

/// In-memory files (browser mode, tests).
pub struct MapProbe<'a>(pub &'a HashMap<String, String>);

impl Probe for MapProbe<'_> {
    fn exists(&self, rel: &str) -> bool {
        let dir = format!("{}/", rel.trim_end_matches('/'));
        self.0.contains_key(rel) || self.0.keys().any(|k| k.starts_with(&dir))
    }
    fn read(&self, rel: &str) -> Option<String> {
        self.0.get(rel).cloned()
    }
    fn files_in(&self, dir: &str) -> Vec<String> {
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let mut out: Vec<String> = self
            .0
            .keys()
            .filter_map(|k| k.strip_prefix(&prefix))
            .filter(|rest| !rest.contains('/'))
            .map(str::to_string)
            .collect();
        out.sort();
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectKind {
    /// A WordPress installation (core files in the project).
    Site,
    /// Bedrock or another Composer-managed installation.
    Composer,
    /// A `wp-content` folder (plugins and themes, core elsewhere).
    Content,
    Plugin,
    Theme,
}

/// How WP-CLI is run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WpCli {
    /// `vendor/bin/wp`, `wp-cli.phar` or the absolute path of `wp` on PATH.
    pub path: String,
    /// `project` or `path`.
    pub source: String,
}

/// A plugin's or theme's file header (the comment block WordPress reads).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_wp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_php: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordPressInfo {
    pub kind: ProjectKind,
    /// The WordPress version of a site (`wp-includes/version.php`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The plugin or theme header, with the file it is in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<Header>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_file: Option<String>,
    /// Whether WordPress core files are in the project (their hooks are indexed).
    pub core: bool,
    /// The installed WordPress stubs file (`php-stubs/wordpress-stubs`), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stubs: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wp_cli: Option<WpCli>,
    /// Indexed hook references and PHP files (filled by the snapshot).
    #[serde(default)]
    pub hooks: usize,
    #[serde(default)]
    pub files: usize,
}

/// Where Composer puts the WordPress stubs.
pub const STUBS_FILE: &str = "vendor/php-stubs/wordpress-stubs/wordpress-stubs.php";
/// The Composer package of the stubs.
pub const STUBS_PACKAGE: &str = "php-stubs/wordpress-stubs";

/// The value of `Key:` in a header comment (` * Plugin Name: Acme`).
fn header_field(src: &str, key: &str) -> Option<String> {
    src.lines().take(80).find_map(|line| {
        let l = line
            .trim_start()
            .trim_start_matches(['*', '#', '/', '@'])
            .trim_start();
        let rest = l.strip_prefix(key)?.trim_start().strip_prefix(':')?;
        let value = rest.trim().trim_end_matches("*/").trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn header(src: &str, name_key: &str) -> Option<Header> {
    Some(Header {
        name: header_field(src, name_key)?,
        version: header_field(src, "Version"),
        text_domain: header_field(src, "Text Domain"),
        requires_wp: header_field(src, "Requires at least"),
        requires_php: header_field(src, "Requires PHP"),
    })
}

/// The header of a plugin's main PHP file.
pub fn plugin_header(src: &str) -> Option<Header> {
    header(src, "Plugin Name")
}

/// The header of a theme's `style.css`.
pub fn theme_header(src: &str) -> Option<Header> {
    header(src, "Theme Name")
}

/// Whether composer.json requires WordPress (`johnpbloch/wordpress*`, `roots/*`).
pub fn composer_requires_wordpress(composer_json: &str) -> bool {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(composer_json) else {
        return false;
    };
    ["require", "require-dev"].iter().any(|s| {
        v.get(s).and_then(|o| o.as_object()).is_some_and(|o| {
            o.keys()
                .any(|k| k.starts_with("johnpbloch/wordpress") || k.starts_with("roots/"))
        })
    })
}

/// `$wp_version = '6.6.2';` from `wp-includes/version.php`.
pub fn version_from_core(src: &str) -> Option<String> {
    let line = src
        .lines()
        .find(|l| l.trim_start().starts_with("$wp_version"))?;
    let start = line.find(['\'', '"'])? + 1;
    let end = start + line[start..].find(['\'', '"'])?;
    Some(line[start..end].to_string())
}

/// Folders a WordPress core install may live in (Bedrock: `web/wp`).
const CORE_DIRS: &[&str] = &["", "wordpress/", "wp/", "web/wp/", "public/"];

/// Detect WordPress; `which` looks a program up on PATH.
pub fn detect(probe: &dyn Probe, which: &dyn Fn(&str) -> Option<String>) -> Option<WordPressInfo> {
    let core_dir = CORE_DIRS.iter().find(|d| {
        probe.exists(&format!("{d}wp-includes/version.php"))
            || probe.exists(&format!("{d}wp-load.php"))
    });
    let mut info = WordPressInfo {
        kind: ProjectKind::Site,
        version: core_dir.and_then(|d| {
            probe
                .read(&format!("{d}wp-includes/version.php"))
                .and_then(|s| version_from_core(&s))
        }),
        header: None,
        header_file: None,
        core: core_dir.is_some(),
        stubs: probe.exists(STUBS_FILE).then(|| STUBS_FILE.to_string()),
        wp_cli: None,
        hooks: 0,
        files: 0,
    };
    let composer = probe
        .read("composer.json")
        .is_some_and(|c| composer_requires_wordpress(&c));
    let site =
        probe.exists("wp-config.php") || probe.exists("wp-config-sample.php") || core_dir.is_some();
    let kind = if composer && !site {
        Some(ProjectKind::Composer)
    } else if site {
        Some(ProjectKind::Site)
    } else if probe.exists("wp-content") {
        Some(ProjectKind::Content)
    } else {
        None
    };
    let root_files = probe.files_in("");
    let plugin = root_files
        .iter()
        .filter(|f| f.ends_with(".php"))
        .find_map(|f| {
            probe
                .read(f)
                .and_then(|s| plugin_header(&s))
                .map(|h| (f.clone(), h))
        });
    let theme = root_files
        .iter()
        .any(|f| f == "style.css")
        .then(|| probe.read("style.css").and_then(|s| theme_header(&s)))
        .flatten();
    match (kind, plugin, theme) {
        (Some(k), ..) => info.kind = k,
        (None, Some((file, h)), _) => {
            info.kind = ProjectKind::Plugin;
            info.header = Some(h);
            info.header_file = Some(file);
        }
        (None, None, Some(h)) => {
            info.kind = ProjectKind::Theme;
            info.header = Some(h);
            info.header_file = Some("style.css".into());
        }
        (None, None, None) => return None,
    }
    info.wp_cli = wp_cli(probe, which);
    Some(info)
}

/// WP-CLI: the project's `vendor/bin/wp` or `wp-cli.phar`, else `wp` on PATH.
pub fn wp_cli(probe: &dyn Probe, which: &dyn Fn(&str) -> Option<String>) -> Option<WpCli> {
    for p in ["vendor/bin/wp", "wp-cli.phar"] {
        if probe.exists(p) {
            return Some(WpCli {
                path: p.into(),
                source: "project".into(),
            });
        }
    }
    which("wp").map(|path| WpCli {
        path,
        source: "path".into(),
    })
}

/// Look `program` up on PATH (with the Windows executable extensions).
pub fn find_on_path(program: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        vec![".exe".into(), ".bat".into(), ".cmd".into(), String::new()]
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path) {
        for ext in &exts {
            let candidate: PathBuf = dir.join(format!("{program}{ext}"));
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(files: &[(&str, &str)]) -> HashMap<String, String> {
        files
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    const PLUGIN: &str = "<?php\n/**\n * Plugin Name:       Acme Events\n * Version:           1.2.0\n * Requires at least: 6.4\n * Requires PHP:      8.1\n * Text Domain:       acme-events\n */\n";

    #[test]
    fn reads_plugin_and_theme_headers() {
        let h = plugin_header(PLUGIN).unwrap();
        assert_eq!(h.name, "Acme Events");
        assert_eq!(h.version.as_deref(), Some("1.2.0"));
        assert_eq!(h.text_domain.as_deref(), Some("acme-events"));
        assert_eq!(h.requires_wp.as_deref(), Some("6.4"));
        assert_eq!(h.requires_php.as_deref(), Some("8.1"));
        let t = theme_header("/*\nTheme Name: Anvil\nVersion: 0.3\n*/").unwrap();
        assert_eq!(
            (t.name.as_str(), t.version.as_deref()),
            ("Anvil", Some("0.3"))
        );
        assert!(plugin_header("<?php echo 1;").is_none());
    }

    #[test]
    fn detects_sites_composer_installs_plugins_and_themes() {
        let none = |_: &str| None;
        let site = map(&[
            ("wp-config.php", "<?php"),
            ("wp-includes/version.php", "<?php\n$wp_version = '6.6.2';\n"),
        ]);
        let info = detect(&MapProbe(&site), &none).unwrap();
        assert_eq!(
            (info.kind, info.version.as_deref(), info.core),
            (ProjectKind::Site, Some("6.6.2"), true)
        );

        let bedrock = map(&[(
            "composer.json",
            r#"{"require": {"roots/wordpress": "^6.6"}}"#,
        )]);
        assert_eq!(
            detect(&MapProbe(&bedrock), &none).unwrap().kind,
            ProjectKind::Composer
        );

        let content = map(&[("wp-content/plugins/a/a.php", "<?php")]);
        assert_eq!(
            detect(&MapProbe(&content), &none).unwrap().kind,
            ProjectKind::Content
        );

        let plugin = map(&[("acme-events.php", PLUGIN), ("readme.txt", "")]);
        let info = detect(&MapProbe(&plugin), &none).unwrap();
        assert_eq!(info.kind, ProjectKind::Plugin);
        assert_eq!(info.header_file.as_deref(), Some("acme-events.php"));
        assert!(!info.core);

        let theme = map(&[
            ("style.css", "/*\nTheme Name: Anvil\n*/"),
            ("functions.php", "<?php"),
        ]);
        assert_eq!(
            detect(&MapProbe(&theme), &none).unwrap().kind,
            ProjectKind::Theme
        );

        let other = map(&[
            (
                "composer.json",
                r#"{"require": {"laravel/framework": "^11"}}"#,
            ),
            ("index.php", "<?php"),
        ]);
        assert!(detect(&MapProbe(&other), &none).is_none());
    }

    #[test]
    fn finds_wp_cli_and_stubs() {
        let files = map(&[
            ("acme.php", PLUGIN),
            ("vendor/bin/wp", "#!/usr/bin/env php"),
            (STUBS_FILE, "<?php"),
        ]);
        let info = detect(&MapProbe(&files), &|_| Some("/usr/bin/wp".into())).unwrap();
        assert_eq!(
            info.wp_cli.unwrap(),
            WpCli {
                path: "vendor/bin/wp".into(),
                source: "project".into()
            }
        );
        assert_eq!(info.stubs.as_deref(), Some(STUBS_FILE));
        let bare = map(&[("acme.php", PLUGIN)]);
        let info = detect(&MapProbe(&bare), &|p| {
            (p == "wp").then(|| "/usr/local/bin/wp".into())
        })
        .unwrap();
        assert_eq!(info.wp_cli.unwrap().source, "path");
        assert!(detect(&MapProbe(&bare), &|_| None)
            .unwrap()
            .wp_cli
            .is_none());
    }

    #[test]
    fn probes_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("acme.php"), PLUGIN).unwrap();
        let info = detect(&DirProbe(dir.path()), &|_| None).unwrap();
        assert_eq!(info.header.unwrap().name, "Acme Events");
    }
}
