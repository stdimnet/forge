//! Completion and documentation in composer.json: package names in `require` /
//! `require-dev` (installed packages, Packagist search, platform packages) and their version
//! constraints (Packagist releases, the installed version), plus the hover of a package.
//!
//! Both work in two steps so the caller can fetch data in between: [`request`] /
//! [`hover_target`] say what the cursor is on, [`items`] / [`hover`] build the answer from
//! what was found (offline: installed.json and the lock file only).

use forge_jsonschema::complete::{json_cursor, Item};
use forge_jsonschema::navigate::{self, Hit};
use forge_jsonschema::text::{byte_offset, Utf16};
use forge_jsonschema::tree::Seg;
use forge_jsonschema::{json, CompletionOut, HoverOut};
use serde::{Deserialize, Serialize};

use crate::constraint::recommended;
use crate::lock::InstalledPackage;
use crate::manifest::{self, is_platform};
use crate::packagist::{PackageInfo, SearchHit};
use crate::version::Version;

const SECTIONS: [&str; 2] = ["require", "require-dev"];

/// What the completion at the cursor needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Request {
    /// A package name in `require` / `require-dev`.
    Packages { prefix: String },
    /// The constraint of `package`.
    Versions { package: String, prefix: String },
}

/// Where the cursor is in composer.json, if completion there is Composer's.
pub fn request(text: &str, offset: usize) -> Option<Request> {
    let byte = byte_offset(text, offset);
    let c = json_cursor(text, byte)?;
    let prefix = text.get(c.from..byte.max(c.from))?.to_string();
    match c.path.as_slice() {
        [Seg::Key(section)] if c.key && SECTIONS.contains(&section.as_str()) => {
            Some(Request::Packages { prefix })
        }
        [Seg::Key(section), Seg::Key(package)]
            if !c.key && SECTIONS.contains(&section.as_str()) =>
        {
            Some(Request::Versions {
                package: package.clone(),
                prefix,
            })
        }
        _ => None,
    }
}

/// What was found for a [`Request`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CompletionData {
    pub installed: Vec<InstalledPackage>,
    /// Packagist search results for the typed prefix.
    pub search: Vec<SearchHit>,
    /// Packagist metadata of the package whose constraint is typed.
    pub package: Option<PackageInfo>,
}

/// Common PHP extensions offered after `ext-`.
const EXTENSIONS: &[&str] = &[
    "bcmath",
    "ctype",
    "curl",
    "dom",
    "fileinfo",
    "filter",
    "gd",
    "iconv",
    "intl",
    "json",
    "mbstring",
    "openssl",
    "pcntl",
    "pdo",
    "pdo_mysql",
    "pdo_pgsql",
    "pdo_sqlite",
    "redis",
    "simplexml",
    "sodium",
    "tokenizer",
    "xml",
    "xmlwriter",
    "zip",
];

fn short(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max - 1).collect();
    format!("{}…", cut.trim_end())
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('$', "\\$")
        .replace('}', "\\}")
}

fn item(label: String, detail: Option<String>, doc: Option<String>, insert: String) -> Item {
    Item {
        label,
        kind: "value",
        detail,
        documentation: doc,
        insert,
        required: false,
        deprecated: false,
        retrigger: false,
    }
}

/// The answer for `request` at UTF-16 `offset` of `text`.
pub fn items(text: &str, offset: usize, data: &CompletionData) -> Option<CompletionOut> {
    let byte = byte_offset(text, offset);
    let c = json_cursor(text, byte)?;
    let req = request(text, offset)?;
    let comma = if c.comma { "," } else { "" };
    let mut out: Vec<Item> = Vec::new();
    match &req {
        Request::Packages { prefix } => {
            let existing: Vec<String> = manifest::requirements(text)
                .into_iter()
                .map(|r| r.name)
                .filter(|n| n != prefix)
                .collect();
            let insert = |name: &str| {
                if c.colon {
                    format!("\"{}\"", escape(name))
                } else {
                    format!("\"{}\": \"$1\"{comma}", escape(name))
                }
            };
            let mut push = |name: &str, detail: Option<String>, doc: Option<String>| {
                if existing.iter().any(|e| e == name) || out.iter().any(|i| i.label == name) {
                    return;
                }
                let mut i = item(name.to_string(), detail, doc, insert(name));
                i.kind = "property";
                i.retrigger = !c.colon;
                out.push(i);
            };
            let lower = prefix.to_ascii_lowercase();
            if lower.starts_with("ext-") {
                for e in EXTENSIONS {
                    push(&format!("ext-{e}"), Some("PHP extension".into()), None);
                }
            } else if "php".starts_with(&lower) || lower.is_empty() {
                push("php", Some("PHP version".into()), None);
            }
            for p in &data.installed {
                if p.name.contains(&lower) || lower.is_empty() {
                    push(
                        &p.name,
                        Some(p.version.clone()),
                        (!p.description.is_empty()).then(|| p.description.clone()),
                    );
                }
            }
            for h in &data.search {
                let mut doc = h.description.clone();
                if h.downloads > 0 {
                    doc.push_str(&format!(
                        "\n\n{} downloads · {} stars",
                        h.downloads, h.favers
                    ));
                }
                push(
                    &h.name,
                    (!h.description.is_empty()).then(|| short(&h.description, 60)),
                    Some(doc),
                );
            }
        }
        Request::Versions { package, .. } => {
            let installed = data.installed.iter().find(|p| &p.name == package);
            let mut push = |label: String, detail: Option<String>| {
                if out.iter().any(|i| i.label == label) {
                    return;
                }
                let insert = format!("\"{}\"{comma}", escape(&label));
                out.push(item(label, detail, None, insert));
            };
            if let Some(info) = &data.package {
                let latest = info.latest();
                if let Some(l) = &latest {
                    push(recommended(l), Some(format!("latest: {}", l.original())));
                }
                // One caret constraint per release line, newest first.
                let mut lines: Vec<(u64, u64)> = Vec::new();
                for v in info.versions.iter().filter_map(|v| Version::parse(v)) {
                    if !v.is_stable() || v.is_branch() {
                        continue;
                    }
                    let line = if v.major() == 0 {
                        (0, v.minor())
                    } else {
                        (v.major(), 0)
                    };
                    if !lines.contains(&line) && lines.len() < 6 {
                        lines.push(line);
                        push(recommended(&v), None);
                    }
                }
                for v in info.versions.iter().take(40) {
                    let detail = installed
                        .filter(|p| &p.version == v)
                        .map(|_| "installed".to_string());
                    push(v.clone(), detail);
                }
            }
            if let Some(p) = installed {
                if let Some(v) = Version::parse(&p.version) {
                    push(recommended(&v), Some(format!("installed: {}", p.version)));
                }
                push(p.version.clone(), Some("installed".into()));
            }
            if package == "php" {
                for v in ["^8.4", "^8.3", "^8.2", "^8.1", ">=8.1"] {
                    push(v.to_string(), None);
                }
            }
            push("*".into(), None);
        }
    }
    if out.is_empty() {
        return None;
    }
    let u = Utf16::new(text);
    Some(CompletionOut {
        from: u.of(c.from),
        replace_from: u.of(c.replace.0),
        replace_to: u.of(c.replace.1),
        items: out,
    })
}

/// The package under UTF-16 `offset` (its key or constraint in `require` /
/// `require-dev`) and the byte range to underline.
pub fn hover_target(text: &str, offset: usize) -> Option<(String, (usize, usize))> {
    let byte = byte_offset(text, offset);
    let doc = json::parse(text)?;
    match navigate::locate(&doc, byte)? {
        Hit::Key(path, key, range) => match path.as_slice() {
            [Seg::Key(s)] if SECTIONS.contains(&s.as_str()) => Some((key.to_string(), range)),
            _ => None,
        },
        Hit::Value(path, node) => match path.as_slice() {
            [Seg::Key(s), Seg::Key(name)] if SECTIONS.contains(&s.as_str()) => {
                Some((name.clone(), (node.start, node.end)))
            }
            _ => None,
        },
    }
    .filter(|(name, _)| !is_platform(name))
}

/// Documentation of `name`: description, installed and latest version and links.
pub fn hover(
    text: &str,
    offset: usize,
    installed: &[InstalledPackage],
    info: Option<&PackageInfo>,
) -> Option<HoverOut> {
    let (name, (start, end)) = hover_target(text, offset)?;
    let local = installed.iter().find(|p| p.name == name);
    let description = info
        .map(|i| i.description.as_str())
        .filter(|d| !d.is_empty())
        .or(local.map(|p| p.description.as_str()))
        .unwrap_or("");
    if local.is_none() && info.is_none() {
        return None;
    }
    let mut md = String::new();
    if !description.is_empty() {
        md.push_str(description);
        md.push_str("\n\n");
    }
    let mut versions = Vec::new();
    if let Some(p) = local {
        versions.push(format!("Installed: `{}`", p.version));
    }
    if let Some(l) = info.and_then(PackageInfo::latest) {
        versions.push(format!("Latest: `{}`", l.original()));
    }
    if !versions.is_empty() {
        md.push_str(&versions.join(" · "));
        md.push_str("\n\n");
    }
    let abandoned = info
        .and_then(|i| i.abandoned.clone())
        .or(local.and_then(|p| p.abandoned.clone()));
    if let Some(r) = abandoned {
        if r.is_empty() {
            md.push_str("**Abandoned.**\n\n");
        } else {
            md.push_str(&format!("**Abandoned**, use `{r}` instead.\n\n"));
        }
    }
    let homepage = info
        .and_then(|i| i.homepage.clone())
        .or(local.and_then(|p| p.homepage.clone()));
    let source = info
        .and_then(|i| i.repository.clone())
        .or(local.and_then(|p| p.source.clone()));
    let mut links = Vec::new();
    if let Some(h) = homepage {
        links.push(format!("[Homepage]({h})"));
    }
    if let Some(s) = source.filter(|s| s.starts_with("http")) {
        links.push(format!("[Source]({s})"));
    }
    links.push(format!(
        "[Packagist](https://packagist.org/packages/{name})"
    ));
    md.push_str(&links.join(" · "));
    let u = Utf16::new(text);
    Some(HoverOut {
        from: u.of(start),
        to: u.of(end),
        title: name,
        markdown: md,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packagist::{fixtures, parse_p2, parse_search};

    fn installed() -> Vec<InstalledPackage> {
        crate::lock::parse_lock(
            r#"{"packages": [{"name": "laravel/framework", "version": "v12.1.0", "description": "The Laravel Framework.",
                "source": {"url": "https://github.com/laravel/framework.git"}},
               {"name": "laravel/prompts", "version": "v0.3.5", "description": "Prompts"}]}"#,
        )
    }

    fn at(text: &str, marker: &str) -> (String, usize) {
        let i = text.find(marker).unwrap();
        (text.replacen(marker, "", 1), i)
    }

    #[test]
    fn completes_package_names() {
        let (text, pos) = at(r#"{"require": {"php": "^8.3", "lara|"}}"#, "|");
        assert_eq!(
            request(&text, pos),
            Some(Request::Packages {
                prefix: "lara".into()
            })
        );
        let data = CompletionData {
            installed: installed(),
            search: parse_search(fixtures::SEARCH).unwrap(),
            package: None,
        };
        let out = items(&text, pos, &data).unwrap();
        let labels: Vec<&str> = out.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "laravel/framework",
                "laravel/prompts",
                "laravel/sanctum",
                "old/thing"
            ]
        );
        assert_eq!(out.items[0].insert, "\"laravel/framework\": \"$1\"");
        assert!(out.items[0].retrigger);
        assert_eq!(&text[out.replace_from..out.replace_to], "\"lara\"");

        // Already required packages are not offered again.
        let (text, pos) = at(r#"{"require": {"laravel/framework": "^12.0", "|"}}"#, "|");
        let out = items(&text, pos, &data).unwrap();
        assert!(!out.items.iter().any(|i| i.label == "laravel/framework"));
        assert!(out.items.iter().any(|i| i.label == "php"));

        let (text, pos) = at(r#"{"require-dev": {"ext-|"}}"#, "|");
        let out = items(&text, pos, &CompletionData::default()).unwrap();
        assert!(out.items.iter().any(|i| i.label == "ext-mbstring"));
    }

    #[test]
    fn completes_versions() {
        let (text, pos) = at(
            r#"{"require": {"laravel/framework": "|", "php": "^8.3"}}"#,
            "|",
        );
        assert_eq!(
            request(&text, pos),
            Some(Request::Versions {
                package: "laravel/framework".into(),
                prefix: String::new()
            })
        );
        let data = CompletionData {
            installed: installed(),
            search: Vec::new(),
            package: Some(parse_p2("laravel/framework", fixtures::LARAVEL_P2).unwrap()),
        };
        let out = items(&text, pos, &data).unwrap();
        let labels: Vec<&str> = out.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "^12.3",
                "^11.44",
                "v13.0.0-beta1",
                "v12.3.0",
                "v12.2.0",
                "v12.1.0",
                "v11.44.0",
                "^12.1",
                "*"
            ]
        );
        assert_eq!(out.items[0].detail.as_deref(), Some("latest: v12.3.0"));
        assert_eq!(out.items[5].detail.as_deref(), Some("installed"));
        assert_eq!(out.items[0].insert, "\"^12.3\"");

        // Offline: the installed version only.
        let offline = CompletionData {
            installed: installed(),
            ..CompletionData::default()
        };
        let out = items(&text, pos, &offline).unwrap();
        assert_eq!(out.items[0].label, "^12.1");

        // Not in require: nothing.
        let (text, pos) = at(r#"{"name": "|"}"#, "|");
        assert_eq!(request(&text, pos), None);
    }

    #[test]
    fn hovers_packages() {
        let text = r#"{"require": {"php": "^8.3", "laravel/framework": "^12.0"}}"#;
        let pos = text.find("framework").unwrap();
        let info = parse_p2("laravel/framework", fixtures::LARAVEL_P2).unwrap();
        let h = hover(text, pos, &installed(), Some(&info)).unwrap();
        assert_eq!(h.title, "laravel/framework");
        assert_eq!(&text[h.from..h.to], "\"laravel/framework\"");
        assert!(h.markdown.starts_with("The Laravel Framework."));
        assert!(h
            .markdown
            .contains("Installed: `v12.1.0` · Latest: `v12.3.0`"));
        assert!(h.markdown.contains("[Homepage](https://laravel.com)"));
        assert!(h
            .markdown
            .contains("[Packagist](https://packagist.org/packages/laravel/framework)"));

        let on_value = text.find("^12").unwrap();
        assert_eq!(hover_target(text, on_value).unwrap().0, "laravel/framework");
        assert!(hover_target(text, text.find("php").unwrap()).is_none());
        assert!(hover(text, pos, &[], None).is_none());
    }
}
