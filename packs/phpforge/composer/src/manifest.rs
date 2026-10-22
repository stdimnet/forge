//! The requirements of a composer.json (`require`, `require-dev`) with where they are
//! written, and helpers for offsets the UI understands.

use forge_jsonschema::json;
use forge_jsonschema::text::Utf16;
use forge_jsonschema::tree::Value;
use serde::Serialize;

/// One `"vendor/package": "constraint"` entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    pub name: String,
    pub constraint: String,
    pub dev: bool,
    /// Byte ranges of the key and the value (quotes included).
    #[serde(skip)]
    pub key: (usize, usize),
    #[serde(skip)]
    pub value: (usize, usize),
}

impl Requirement {
    /// `php`, `ext-*`, `lib-*`, `composer-plugin-api`, …: provided by the platform, not
    /// packages on Packagist.
    pub fn is_platform(&self) -> bool {
        is_platform(&self.name)
    }
}

pub fn is_platform(name: &str) -> bool {
    !name.contains('/')
        && (name == "php"
            || name.starts_with("php-")
            || name.starts_with("ext-")
            || name.starts_with("lib-")
            || name == "hhvm"
            || name.starts_with("composer"))
}

/// A valid Packagist package name (`vendor/package`, lowercase), the only names Forge asks
/// Packagist (and its cache) about.
pub fn is_package_name(name: &str) -> bool {
    let Some((vendor, package)) = name.split_once('/') else {
        return false;
    };
    let part = |s: &str| {
        !s.is_empty()
            && s.len() <= 100
            && s.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            && s.ends_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            && s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c))
            && !s.contains("..")
    };
    part(vendor) && part(package)
}

/// The requirements of a composer.json text, in file order (`require` first).
pub fn requirements(text: &str) -> Vec<Requirement> {
    let Some(doc) = json::parse(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (section, dev) in [("require", false), ("require-dev", true)] {
        let Some(Value::Object(props)) = doc.get(section).map(|n| &n.value) else {
            continue;
        };
        for p in props {
            let Some(v) = &p.value else { continue };
            let Value::String(constraint) = &v.value else {
                continue;
            };
            out.push(Requirement {
                name: p.key.clone(),
                constraint: constraint.clone(),
                dev,
                key: (p.key_start, p.key_end),
                value: (v.start, v.end),
            });
        }
    }
    out
}

/// The byte range of `"name": "<name>"` in a composer.lock, for problems in the lock file.
pub fn lock_entry(lock: &str, name: &str) -> Option<(usize, usize)> {
    let doc = json::parse(lock)?;
    for key in ["packages", "packages-dev"] {
        let Some(Value::Array(items)) = doc.get(key).map(|n| &n.value) else {
            continue;
        };
        for item in items {
            if let Some(n) = item.get("name").filter(|n| n.as_str() == Some(name)) {
                return Some((n.start, n.end));
            }
        }
    }
    None
}

/// A position as the UI's editors and the Problems tool window take it: 0-based line and
/// UTF-16 column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub line: usize,
    pub character: usize,
}

/// Converts byte offsets of one text to UTF-16 offsets and line/column positions.
pub struct Offsets<'a> {
    text: &'a str,
    utf16: Utf16,
    line_starts: Vec<usize>,
}

impl<'a> Offsets<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Offsets {
            text,
            utf16: Utf16::new(text),
            line_starts,
        }
    }

    pub fn utf16(&self, byte: usize) -> usize {
        self.utf16.of(byte.min(self.text.len()))
    }

    pub fn line(&self, byte: usize) -> usize {
        self.line_starts.partition_point(|s| *s <= byte) - 1
    }

    pub fn position(&self, byte: usize) -> Position {
        let line = self.line(byte);
        let start = self.line_starts[line];
        Position {
            line,
            character: self.utf16(byte) - self.utf16(start),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"{
    "require": {
        "php": "^8.3",
        "laravel/framework": "^12.0"
    },
    "require-dev": {"phpunit/phpunit": "^11.0", "broken": 1}
}"#;

    #[test]
    fn finds_requirements() {
        let r = requirements(JSON);
        let names: Vec<&str> = r.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["php", "laravel/framework", "phpunit/phpunit"]);
        assert!(r[0].is_platform() && !r[1].is_platform());
        assert!(r[2].dev);
        assert_eq!(&JSON[r[1].value.0..r[1].value.1], "\"^12.0\"");
        assert_eq!(&JSON[r[1].key.0..r[1].key.1], "\"laravel/framework\"");
        assert!(requirements("not json").is_empty());
    }

    #[test]
    fn validates_package_names() {
        assert!(is_package_name("laravel/framework"));
        assert!(is_package_name("symfony/polyfill-php83"));
        assert!(is_package_name("doctrine/dbal"));
        for bad in [
            "Laravel/framework",
            "php",
            "a/../b",
            "../x/y",
            "a/b/c",
            "a/",
            "a b/c",
        ] {
            assert!(!is_package_name(bad), "{bad}");
        }
        assert!(is_platform("ext-json") && is_platform("composer-plugin-api"));
        assert!(!is_platform("composer/composer"));
    }

    #[test]
    fn positions_count_utf16() {
        let text = "{\n  \"é\": \"x\"\n}";
        let o = Offsets::new(text);
        let x = text.find('x').unwrap();
        assert_eq!(
            o.position(x),
            Position {
                line: 1,
                character: 8
            }
        );
        assert_eq!(o.utf16(x), 10);
    }

    #[test]
    fn finds_lock_entries() {
        let lock =
            r#"{"packages": [{"name": "a/b"}], "packages-dev": [{"name": "c/d", "version": "1"}]}"#;
        let (s, e) = lock_entry(lock, "c/d").unwrap();
        assert_eq!(&lock[s..e], "\"c/d\"");
        assert!(lock_entry(lock, "x/y").is_none());
    }
}
