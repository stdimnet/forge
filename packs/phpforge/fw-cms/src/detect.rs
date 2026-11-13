//! Drupal, Joomla and Shopware detection: a full site or a single extension (a Drupal
//! module or theme with its `.info.yml`, a Joomla extension manifest, a Shopware plugin),
//! with the version and the console the IDE runs (`drush`, `bin/console`).

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Read access to the project's files, relative paths with `/` separators.
pub trait Probe {
    fn exists(&self, rel: &str) -> bool;
    fn read(&self, rel: &str) -> Option<String>;
    /// File names directly in `dir` ("" for the root).
    fn files_in(&self, dir: &str) -> Vec<String>;
}

pub struct DirProbe<'a>(pub &'a Path);

impl Probe for DirProbe<'_> {
    fn exists(&self, rel: &str) -> bool {
        self.0.join(rel).exists()
    }
    fn read(&self, rel: &str) -> Option<String> {
        let bytes = std::fs::read(self.0.join(rel)).ok()?;
        let end = bytes.len().min(256 * 1024);
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CmsInfo {
    /// `drupal`, `joomla` or `shopware` (the framework id of project detection).
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// `site` or `extension` (a module, theme, component or plugin on its own).
    pub kind: String,
    /// The extension's name or manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    /// The console the run actions use: `vendor/bin/drush`, `drush` or `bin/console`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub console: Option<String>,
}

fn composer_packages(probe: &dyn Probe) -> (Vec<String>, Option<String>, HashMap<String, String>) {
    let mut names = Vec::new();
    let mut kind = None;
    if let Some(v) = probe
        .read("composer.json")
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    {
        kind = v.get("type").and_then(|t| t.as_str()).map(str::to_string);
        for s in ["require", "require-dev"] {
            if let Some(o) = v.get(s).and_then(|o| o.as_object()) {
                names.extend(o.keys().cloned());
            }
        }
    }
    let mut locked = HashMap::new();
    if let Some(v) = probe
        .read("composer.lock")
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    {
        for s in ["packages", "packages-dev"] {
            for p in v.get(s).and_then(|a| a.as_array()).into_iter().flatten() {
                if let (Some(n), Some(ver)) = (p["name"].as_str(), p["version"].as_str()) {
                    locked.insert(n.to_string(), ver.trim_start_matches('v').to_string());
                }
            }
        }
    }
    (names, kind, locked)
}

/// `const VERSION = '10.3.1';` and the like.
fn quoted_after(src: &str, marker: &str) -> Option<String> {
    let at = src.find(marker)? + marker.len();
    let rest = &src[at..];
    let start = rest.find(['\'', '"'])? + 1;
    let end = start + rest[start..].find(['\'', '"'])?;
    Some(rest[start..end].to_string())
}

fn xml_tag(src: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let start = src.find(&open)? + open.len();
    let end = start + src[start..].find(&format!("</{tag}>"))?;
    Some(src[start..end].trim().to_string())
}

fn drupal(
    probe: &dyn Probe,
    packages: &[String],
    locked: &HashMap<String, String>,
    which: &dyn Fn(&str) -> Option<String>,
) -> Option<CmsInfo> {
    let core = [
        "core/lib/Drupal.php",
        "web/core/lib/Drupal.php",
        "docroot/core/lib/Drupal.php",
    ]
    .into_iter()
    .find(|p| probe.exists(p));
    let composer = packages
        .iter()
        .any(|p| p.starts_with("drupal/core") || p == "drupal/recommended-project");
    let info_file = probe
        .files_in("")
        .into_iter()
        .find(|f| f.ends_with(".info.yml"))
        .filter(|f| {
            probe
                .read(f)
                .is_some_and(|s| s.contains("core_version_requirement") || s.contains("type:"))
        });
    if core.is_none() && !composer && info_file.is_none() {
        return None;
    }
    let version = core
        .and_then(|c| probe.read(c))
        .and_then(|s| quoted_after(&s, "const VERSION"))
        .or_else(|| locked.get("drupal/core").cloned())
        .or_else(|| locked.get("drupal/core-recommended").cloned());
    let console = if probe.exists("vendor/bin/drush") {
        Some("vendor/bin/drush".to_string())
    } else {
        which("drush").map(|_| "drush".to_string())
    };
    // A module with its own composer.json (requiring drupal/core) is still an extension.
    let site = core.is_some() || (composer && info_file.is_none());
    Some(CmsInfo {
        id: "drupal".into(),
        name: "Drupal".into(),
        version,
        kind: if site { "site" } else { "extension" }.into(),
        extension: (!site)
            .then(|| info_file.clone())
            .flatten()
            .map(|f| f.trim_end_matches(".info.yml").to_string()),
        console,
    })
}

fn joomla(probe: &dyn Probe) -> Option<CmsInfo> {
    let manifest = probe.read("administrator/manifests/files/joomla.xml");
    let version_php = probe.read("libraries/src/Version.php");
    if manifest.is_some() || version_php.as_ref().is_some_and(|s| s.contains("Joomla")) {
        let version = manifest.as_deref().and_then(|m| xml_tag(m, "version"));
        return Some(CmsInfo {
            id: "joomla".into(),
            name: "Joomla".into(),
            version,
            kind: "site".into(),
            extension: None,
            console: probe
                .exists("cli/joomla.php")
                .then(|| "cli/joomla.php".to_string()),
        });
    }
    // An extension: a root XML manifest `<extension type="component" …>`.
    probe
        .files_in("")
        .into_iter()
        .filter(|f| f.ends_with(".xml"))
        .find_map(|f| {
            let src = probe.read(&f)?;
            let head = src.get(..src.len().min(2000))?;
            (head.contains("<extension")
                && (head.contains("type=\"component\"")
                    || head.contains("type=\"module\"")
                    || head.contains("type=\"plugin\"")
                    || head.contains("type=\"template\"")
                    || head.contains("type=\"package\"")
                    || head.contains("type=\"library\"")))
            .then(|| CmsInfo {
                id: "joomla".into(),
                name: "Joomla".into(),
                version: None,
                kind: "extension".into(),
                extension: xml_tag(&src, "name").or(Some(f)),
                console: None,
            })
        })
}

fn shopware(
    probe: &dyn Probe,
    packages: &[String],
    kind: Option<&str>,
    locked: &HashMap<String, String>,
) -> Option<CmsInfo> {
    let six = packages.iter().any(|p| {
        matches!(
            p.as_str(),
            "shopware/core"
                | "shopware/platform"
                | "shopware/storefront"
                | "shopware/administration"
        )
    });
    let plugin = kind == Some("shopware-platform-plugin");
    let five = probe.exists("engine/Shopware/Kernel.php")
        || probe.exists("engine/Shopware/Application.php");
    if !six && !plugin && !five {
        return None;
    }
    let version = if five && !six {
        probe
            .read("engine/Shopware/Kernel.php")
            .and_then(|s| quoted_after(&s, "'version' =>"))
            .or(Some("5".into()))
    } else {
        locked
            .get("shopware/core")
            .or_else(|| locked.get("shopware/platform"))
            .cloned()
    };
    Some(CmsInfo {
        id: "shopware".into(),
        name: "Shopware".into(),
        version,
        kind: if plugin { "extension" } else { "site" }.into(),
        extension: None,
        console: probe
            .exists("bin/console")
            .then(|| "bin/console".to_string()),
    })
}

/// Every CMS the project uses (usually one).
pub fn detect(probe: &dyn Probe, which: &dyn Fn(&str) -> Option<String>) -> Vec<CmsInfo> {
    let (packages, kind, locked) = composer_packages(probe);
    [
        drupal(probe, &packages, &locked, which),
        joomla(probe),
        shopware(probe, &packages, kind.as_deref(), &locked),
    ]
    .into_iter()
    .flatten()
    .collect()
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

    fn ids(files: &[(&str, &str)]) -> Vec<(String, String, Option<String>)> {
        detect(&MapProbe(&map(files)), &|_| None)
            .into_iter()
            .map(|c| (c.id, c.kind, c.version))
            .collect()
    }

    #[test]
    fn detects_drupal_sites_and_modules() {
        let site = [
            (
                "composer.json",
                r#"{"require": {"drupal/core-recommended": "^10.3"}}"#,
            ),
            (
                "web/core/lib/Drupal.php",
                "<?php\nclass Drupal {\n  const VERSION = '10.3.1';\n}",
            ),
            ("vendor/bin/drush", ""),
        ];
        assert_eq!(
            ids(&site),
            vec![("drupal".into(), "site".into(), Some("10.3.1".into()))]
        );
        let info = &detect(&MapProbe(&map(&site)), &|_| None)[0];
        assert_eq!(info.console.as_deref(), Some("vendor/bin/drush"));
        let module = [
            (
                "acme.info.yml",
                "name: Acme\ntype: module\ncore_version_requirement: ^10\n",
            ),
            ("acme.module", "<?php"),
        ];
        let info = &detect(&MapProbe(&map(&module)), &|p| {
            (p == "drush").then(|| "/usr/bin/drush".into())
        })[0];
        assert_eq!(
            (
                info.kind.as_str(),
                info.extension.as_deref(),
                info.console.as_deref()
            ),
            ("extension", Some("acme"), Some("drush"))
        );
    }

    #[test]
    fn detects_joomla_sites_and_extensions() {
        let site = [
            (
                "administrator/manifests/files/joomla.xml",
                "<extension><version>5.1.2</version></extension>",
            ),
            ("cli/joomla.php", ""),
        ];
        assert_eq!(
            ids(&site),
            vec![("joomla".into(), "site".into(), Some("5.1.2".into()))]
        );
        let ext = [("com_acme.xml", "<?xml version=\"1.0\"?>\n<extension type=\"component\" method=\"upgrade\">\n<name>COM_ACME</name>\n</extension>")];
        let info = &detect(&MapProbe(&map(&ext)), &|_| None)[0];
        assert_eq!(
            (info.kind.as_str(), info.extension.as_deref()),
            ("extension", Some("COM_ACME"))
        );
        assert!(ids(&[("phpunit.xml", "<phpunit/>")]).is_empty());
    }

    #[test]
    fn detects_shopware_5_and_6() {
        let six = [
            (
                "composer.json",
                r#"{"require": {"shopware/core": "~6.6.0"}}"#,
            ),
            (
                "composer.lock",
                r#"{"packages": [{"name": "shopware/core", "version": "v6.6.4.1"}]}"#,
            ),
            ("bin/console", ""),
        ];
        let info = &detect(&MapProbe(&map(&six)), &|_| None)[0];
        assert_eq!(
            (
                info.id.as_str(),
                info.version.as_deref(),
                info.console.as_deref()
            ),
            ("shopware", Some("6.6.4.1"), Some("bin/console"))
        );
        let plugin = [(
            "composer.json",
            r#"{"type": "shopware-platform-plugin", "require": {}}"#,
        )];
        assert_eq!(ids(&plugin)[0].1, "extension");
        let five = [(
            "engine/Shopware/Kernel.php",
            "<?php\n'version' => '5.7.19',",
        )];
        assert_eq!(ids(&five)[0].2.as_deref(), Some("5.7.19"));
    }
}
