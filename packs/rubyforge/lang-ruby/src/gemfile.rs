//! Bundler's files: the `Gemfile` (sources, `ruby`, `gemspec`, `gem` declarations with
//! requirements, groups from `group … do` blocks and `group:` options, `require: false`,
//! `path:` / `git:` / `github:` sources) and `Gemfile.lock` (`GEM`, `GIT`, `PATH`
//! sections with their specs and dependencies, `PLATFORMS`, `DEPENDENCIES`, `RUBY
//! VERSION`, `BUNDLED WITH`). [`bundle`] merges both into what the Bundler tool window and
//! External Libraries show.
//!
//! The Gemfile is Ruby; Forge reads it without running it (a tolerant line scanner), so
//! declarations built at run time (loops, `eval_gemfile`) are not seen.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A `gem` line of the Gemfile.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GemDeclaration {
    pub name: String,
    /// Version requirements (`~> 7.1`, `>= 1.2`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<String>,
    /// Groups (`default` when none).
    pub groups: Vec<String>,
    /// 0-based line.
    pub line: u32,
    /// `require: false`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_require: bool,
    /// `path:`, `git:` or `github:` source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gemfile {
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ruby: Option<String>,
    /// The Gemfile loads the project's gemspec (`gemspec`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gemspec: bool,
    pub gems: Vec<GemDeclaration>,
}

/// Quoted strings and `key: value` / `:key => value` options of a call's arguments.
fn arguments(rest: &str) -> (Vec<String>, BTreeMap<String, String>) {
    let mut strings = Vec::new();
    let mut options = BTreeMap::new();
    let rest = rest.trim().trim_start_matches('(');
    let rest = rest.split(" do").next().unwrap_or(rest);
    for part in split_top(rest) {
        let p = part.trim().trim_end_matches(')').trim();
        if p.is_empty() {
            continue;
        }
        let kv = p
            .split_once("=>")
            .map(|(k, v)| {
                (
                    k.trim().trim_start_matches(':').trim_matches(['"', '\'']),
                    v,
                )
            })
            .or_else(|| {
                let (k, v) = p.split_once(':')?;
                (!k.contains(['"', '\'', ' '])).then_some((k.trim(), v))
            });
        match kv {
            Some((k, v)) if !k.is_empty() && !p.starts_with(['"', '\'']) => {
                options.insert(k.to_string(), v.trim().to_string());
            }
            _ => {
                if let Some(s) = unquote(p) {
                    strings.push(s);
                }
            }
        }
    }
    (strings, options)
}

/// Split on commas outside quotes and brackets.
fn split_top(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut depth = 0i32;
    for c in s.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '[' | '{' | '(' => depth += 1,
                ']' | '}' | ')' => depth -= 1,
                '#' if depth == 0 => break,
                ',' if depth == 0 => {
                    out.push(std::mem::take(&mut cur));
                    continue;
                }
                _ => {}
            },
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn unquote(s: &str) -> Option<String> {
    let s = s.trim();
    let q = s.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let inner = s[1..].strip_suffix(q)?;
    Some(inner.to_string())
}

/// `:test, :development` / `[:test, :development]` → names.
fn symbols(s: &str) -> Vec<String> {
    let s = s.trim();
    let s = s
        .strip_prefix("%i")
        .or_else(|| s.strip_prefix("%w"))
        .unwrap_or(s);
    s.trim_start_matches('[')
        .trim_end_matches(']')
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(|p| {
            p.trim()
                .trim_start_matches(':')
                .trim_matches(['"', '\''])
                .to_string()
        })
        .filter(|p| !p.is_empty() && p.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .collect()
}

/// An open block of the Gemfile: the groups of a `group` block, the platforms of a
/// `platforms` block (both `None` for other blocks).
type Block = (Option<Vec<String>>, Option<Vec<String>>);

/// Parse a Gemfile.
pub fn parse_gemfile(text: &str) -> Gemfile {
    let mut g = Gemfile::default();
    let mut blocks: Vec<Block> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let t = raw.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let word = t
            .split(|c: char| c.is_whitespace() || c == '(')
            .next()
            .unwrap_or("");
        let rest = &t[word.len()..];
        let opens = t.ends_with(" do") || t.contains(" do |") || t.ends_with(" do;");
        match word {
            "source" => {
                if let Some(s) = arguments(rest).0.into_iter().next() {
                    g.sources.push(s);
                }
                if opens {
                    blocks.push((None, None));
                }
            }
            "ruby" => {
                g.ruby = crate::version::parse_gemfile_ruby(t).and_then(Result::ok);
            }
            "gemspec" => g.gemspec = true,
            "group" => {
                let groups = symbols(rest.split(" do").next().unwrap_or(rest));
                blocks.push((Some(groups), None));
            }
            "platforms" | "platform" => {
                let p = symbols(rest.split(" do").next().unwrap_or(rest));
                blocks.push((None, Some(p)));
            }
            "gem" => {
                let (strings, options) = arguments(rest);
                let Some(name) = strings.first().cloned() else {
                    continue;
                };
                let mut groups: Vec<String> = blocks
                    .iter()
                    .filter_map(|(g, _)| g.clone())
                    .flatten()
                    .collect();
                for key in ["group", "groups"] {
                    if let Some(v) = options.get(key) {
                        groups.extend(symbols(v));
                    }
                }
                if groups.is_empty() {
                    groups.push("default".into());
                }
                groups.dedup();
                let mut platforms: Vec<String> = blocks
                    .iter()
                    .filter_map(|(_, p)| p.clone())
                    .flatten()
                    .collect();
                for key in ["platform", "platforms"] {
                    if let Some(v) = options.get(key) {
                        platforms.extend(symbols(v));
                    }
                }
                let source = ["path", "git", "github"].iter().find_map(|k| {
                    options
                        .get(*k)
                        .and_then(|v| unquote(v))
                        .map(|v| format!("{k}: {v}"))
                });
                g.gems.push(GemDeclaration {
                    name,
                    requirements: strings[1..].to_vec(),
                    groups,
                    line: i as u32,
                    no_require: options.get("require").is_some_and(|v| v.trim() == "false"),
                    source,
                    platforms,
                });
                if opens {
                    blocks.push((None, None));
                }
            }
            "end" => {
                blocks.pop();
            }
            _ => {
                if opens {
                    blocks.push((None, None));
                }
            }
        }
    }
    g
}

/// A gem of the lock file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedSpec {
    pub name: String,
    pub version: String,
    /// `x86_64-linux`, `arm64-darwin` for native gems.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// Names of the gems it depends on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
    /// `rubygems` (`GEM`), `git` or `path`.
    pub source: String,
    /// The remote (`https://rubygems.org/`, a git URL, a path).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub remote: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lockfile {
    pub specs: Vec<LockedSpec>,
    pub platforms: Vec<String>,
    /// Direct dependencies (`DEPENDENCIES`), `!` marks gems from git / path sources.
    pub dependencies: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ruby_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundled_with: Option<String>,
}

impl Lockfile {
    pub fn spec(&self, name: &str) -> Option<&LockedSpec> {
        self.specs.iter().find(|s| s.name == name)
    }
}

/// `rails (7.1.3.4)` / `nokogiri (1.16.7-x86_64-linux)` → (name, version, platform).
fn spec_line(s: &str) -> Option<(String, String, Option<String>)> {
    let (name, rest) = s.trim().split_once(" (")?;
    let v = rest.strip_suffix(')')?;
    // Platforms follow the version after a `-` that starts a non-numeric part.
    let (version, platform) = match v.find('-') {
        Some(i) if v[i + 1..].starts_with(|c: char| c.is_ascii_alphabetic()) => {
            (v[..i].to_string(), Some(v[i + 1..].to_string()))
        }
        _ => (v.to_string(), None),
    };
    Some((name.to_string(), version, platform))
}

/// Parse a Gemfile.lock.
pub fn parse_lockfile(text: &str) -> Lockfile {
    let mut lock = Lockfile::default();
    let mut section = String::new();
    let mut remote = String::new();
    let mut in_specs = false;
    for raw in text.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        let t = raw.trim();
        if indent == 0 {
            section = t.to_string();
            remote.clear();
            in_specs = false;
            continue;
        }
        match section.as_str() {
            "GEM" | "GIT" | "PATH" => {
                if indent == 2 {
                    if let Some(r) = t.strip_prefix("remote: ") {
                        remote = r.to_string();
                    }
                    in_specs = t == "specs:";
                } else if in_specs && indent == 4 {
                    if let Some((name, version, platform)) = spec_line(t) {
                        lock.specs.push(LockedSpec {
                            name,
                            version,
                            platform,
                            dependencies: Vec::new(),
                            source: match section.as_str() {
                                "GEM" => "rubygems".into(),
                                s => s.to_lowercase(),
                            },
                            remote: remote.clone(),
                        });
                    }
                } else if in_specs && indent >= 6 {
                    if let Some(last) = lock.specs.last_mut() {
                        let dep = t.split(" (").next().unwrap_or(t).to_string();
                        if !last.dependencies.contains(&dep) {
                            last.dependencies.push(dep);
                        }
                    }
                }
            }
            "PLATFORMS" => lock.platforms.push(t.to_string()),
            "DEPENDENCIES" => {
                let name = t.split(" (").next().unwrap_or(t).trim_end_matches('!');
                lock.dependencies.push(name.to_string());
            }
            "RUBY VERSION" => {
                lock.ruby_version = Some(t.trim_start_matches("ruby ").to_string());
            }
            "BUNDLED WITH" => lock.bundled_with = Some(t.to_string()),
            _ => {}
        }
    }
    // A gem locked for several platforms counts once (the plain one first).
    let mut seen = std::collections::BTreeSet::new();
    lock.specs.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(a.platform.is_some().cmp(&b.platform.is_some()))
    });
    lock.specs.retain(|s| seen.insert(s.name.clone()));
    lock
}

/// A gem as the Bundler tool window shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleGem {
    pub name: String,
    /// The locked version (empty when the lock file does not have it yet).
    pub version: String,
    /// Declared in the Gemfile (direct) or pulled in by another gem.
    pub direct: bool,
    pub groups: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
    /// 0-based Gemfile line of a direct gem.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub source: String,
}

/// The Gemfile and its lock file together.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    pub gemfile: Gemfile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<Lockfile>,
    /// Direct gems first (Gemfile order), then the others by name.
    pub gems: Vec<BundleGem>,
    /// Gems of the Gemfile the lock file lacks: `bundle install` is due.
    pub missing: Vec<String>,
}

impl Bundle {
    /// Whether a gem is part of the bundle (`rspec-core` for RSpec, `minitest`, `rubocop`).
    pub fn has(&self, name: &str) -> bool {
        self.gems.iter().any(|g| g.name == name)
    }

    pub fn gem(&self, name: &str) -> Option<&BundleGem> {
        self.gems.iter().find(|g| g.name == name)
    }
}

/// Merge a Gemfile with its lock file (`None`: not locked yet).
pub fn bundle(gemfile: &str, lock: Option<&str>) -> Bundle {
    let g = parse_gemfile(gemfile);
    let lock = lock.map(parse_lockfile);
    let mut gems = Vec::new();
    let mut missing = Vec::new();
    for d in &g.gems {
        let spec = lock.as_ref().and_then(|l| l.spec(&d.name));
        if lock.is_some() && spec.is_none() && d.platforms.is_empty() {
            missing.push(d.name.clone());
        }
        gems.push(BundleGem {
            name: d.name.clone(),
            version: spec.map(|s| s.version.clone()).unwrap_or_default(),
            direct: true,
            groups: d.groups.clone(),
            requirements: d.requirements.clone(),
            dependencies: spec.map(|s| s.dependencies.clone()).unwrap_or_default(),
            line: Some(d.line),
            source: spec
                .map(|s| s.source.clone())
                .or_else(|| {
                    d.source
                        .as_ref()
                        .map(|s| s.split(':').next().unwrap_or("").to_string())
                })
                .unwrap_or_else(|| "rubygems".into()),
        });
    }
    if let Some(l) = &lock {
        // Groups of transitive gems: those of the direct gems that pull them in.
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut queue: Vec<(String, Vec<String>)> = gems
            .iter()
            .map(|g| (g.name.clone(), g.groups.clone()))
            .collect();
        while let Some((name, gs)) = queue.pop() {
            let Some(spec) = l.spec(&name) else { continue };
            for dep in &spec.dependencies {
                let entry = groups.entry(dep.clone()).or_default();
                let before = entry.len();
                for g in &gs {
                    if !entry.contains(g) {
                        entry.push(g.clone());
                    }
                }
                if entry.len() != before {
                    queue.push((dep.clone(), entry.clone()));
                }
            }
        }
        for s in &l.specs {
            if gems.iter().any(|g| g.name == s.name) {
                continue;
            }
            let mut gs = groups.get(&s.name).cloned().unwrap_or_default();
            if gs.is_empty() {
                gs.push("default".into());
            }
            gs.sort();
            gems.push(BundleGem {
                name: s.name.clone(),
                version: s.version.clone(),
                direct: false,
                groups: gs,
                requirements: Vec::new(),
                dependencies: s.dependencies.clone(),
                line: None,
                source: s.source.clone(),
            });
        }
    }
    Bundle {
        gemfile: g,
        lock,
        gems,
        missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    #[test]
    fn gemfile_declarations_and_groups() {
        let g = parse_gemfile(&fixture("shop/Gemfile"));
        assert_eq!(g.sources, ["https://rubygems.org"]);
        assert_eq!(g.ruby.as_deref(), Some("3.3.6"));
        let rows: Vec<_> = g
            .gems
            .iter()
            .map(|d| {
                (
                    d.name.as_str(),
                    d.requirements.join(", "),
                    d.groups.join(","),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                ("rake", "~> 13.0".to_string(), "default".to_string()),
                ("money", ">= 6.16, < 7".into(), "default".into()),
                ("tzinfo-data", "".into(), "default".into()),
                ("debug", "".into(), "development,test".into()),
                ("rubocop", "~> 1.66".into(), "development,test".into()),
                ("rspec", "~> 3.13".into(), "test".into()),
                ("minitest", "".into(), "test".into()),
                ("simplecov", "".into(), "test,coverage".into()),
                ("pry", "".into(), "development".into()),
                ("shop_core", "".into(), "default".into()),
            ]
        );
        let tz = &g.gems[2];
        assert_eq!(tz.platforms, ["windows", "jruby"]);
        assert!(g.gems[4].no_require);
        assert_eq!(g.gems[9].source.as_deref(), Some("path: engines/shop_core"));
        assert_eq!(g.gems[0].line, 4);
    }

    #[test]
    fn lockfile_sections() {
        let l = parse_lockfile(&fixture("shop/Gemfile.lock"));
        assert_eq!(l.platforms, ["arm64-darwin", "x86_64-linux"]);
        assert_eq!(l.ruby_version.as_deref(), Some("3.3.6p108"));
        assert_eq!(l.bundled_with.as_deref(), Some("2.5.22"));
        assert!(l.dependencies.contains(&"shop_core".to_string()));
        let rubocop = l.spec("rubocop").unwrap();
        assert_eq!(rubocop.version, "1.66.1");
        assert!(rubocop.dependencies.contains(&"parser".to_string()));
        assert_eq!(l.spec("shop_core").unwrap().source, "path");
        // Native gems: one entry with the plain version.
        let n: Vec<_> = l.specs.iter().filter(|s| s.name == "nokogiri").collect();
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].version, "1.16.7");
    }

    #[test]
    fn bundle_merges_groups_and_finds_missing_gems() {
        let b = bundle(
            &fixture("shop/Gemfile"),
            Some(&fixture("shop/Gemfile.lock")),
        );
        assert!(b.has("rspec-core") && b.has("minitest") && b.has("rubocop"));
        let parser = b.gem("parser").unwrap();
        assert!(!parser.direct);
        assert_eq!(parser.groups, ["development", "test"]);
        assert_eq!(b.gem("rspec").unwrap().version, "3.13.0");
        assert_eq!(b.missing, ["pry"]);
        let unlocked = bundle("gem 'rack'\n", None);
        assert!(unlocked.missing.is_empty());
        assert_eq!(unlocked.gems[0].version, "");
    }

    #[test]
    fn argument_scanner() {
        let (s, o) = arguments(" \"rails\", \"~> 7.1\", require: false, group: [:a, :b] # x");
        assert_eq!(s, ["rails", "~> 7.1"]);
        assert_eq!(o["require"], "false");
        assert_eq!(symbols(&o["group"]), ["a", "b"]);
        let (_, o) = arguments("('x', :git => 'https://g/x.git')");
        assert_eq!(unquote(&o["git"]).as_deref(), Some("https://g/x.git"));
    }
}
