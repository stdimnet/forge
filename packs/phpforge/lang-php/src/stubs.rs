//! PHP's own classes, functions and constants from stub files, like PHPStorm's "PHP Runtime":
//! JetBrains' phpstorm-stubs (Apache-2.0; one folder per extension such as `Core/Core_c.php`,
//! `standard/standard_9.php`, `SPL/SPL.php`) or a compatible layout. Go to Declaration on `\Throwable` or `array_map` opens the stub read-only.
//!
//! The index is built once per stubs folder; lookups answer the [`ExternalSymbol`]s that
//! [`crate::navigate`] cannot resolve in the project. Paths are relative to the stubs
//! folder with `/` separators.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::hover::word_at;
use crate::model::{short_name, ClassLike, FileIndex, Member, Range, SymbolKind};
use crate::navigate::{ExternalMember, ExternalSymbol, SymbolInfo};
use crate::parser;

/// A declaration in the stubs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StubTarget {
    /// Relative to the stubs folder, e.g. `Core/Core_c.php`.
    pub path: String,
    /// 0-based line and column of the name.
    pub line: u32,
    pub column: u32,
    /// `Throwable`, `Exception::getMessage()`, `array_map()`, `PHP_EOL`.
    pub name: String,
    /// The extension (the stub's folder): `Core`, `standard`, `SPL`.
    pub extension: String,
    pub info: SymbolInfo,
}

#[derive(Debug, Default)]
pub struct StubIndex {
    root: PathBuf,
    files: HashMap<String, FileIndex>,
    /// Lowercase FQN -> file.
    classes: HashMap<String, String>,
    /// Lowercase FQN -> (file, function index).
    functions: HashMap<String, (String, usize)>,
    /// FQN (case-sensitive) -> (file, name range, value).
    constants: HashMap<String, (String, Range, Option<String>)>,
}

/// Folders of the phpstorm-stubs repository that hold no stubs.
const SKIP_DIRS: &[&str] = &["tests", "meta", ".github", ".git", "vendor", "node_modules"];

fn extension_of(path: &str) -> String {
    path.split('/')
        .next()
        .filter(|_| path.contains('/'))
        .unwrap_or("Core")
        .to_string()
}

/// A `define()` call: name, byte offset of the name, value.
type Define = (String, usize, Option<String>);

/// `define('NAME', value)` calls (most of the stubs' constants), as (name, byte offset of
/// the name, value).
fn defines(src: &str) -> Vec<Define> {
    let mut out = Vec::new();
    for (i, _) in src.match_indices("define") {
        if src[..i]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '>' || c == ':')
        {
            continue;
        }
        let rest = src[i + 6..].trim_start();
        let Some(args) = rest.strip_prefix('(') else {
            continue;
        };
        let args_trim = args.trim_start();
        let Some(quote) = args_trim.chars().next().filter(|c| *c == '\'' || *c == '"') else {
            continue;
        };
        let body = &args_trim[1..];
        let Some(close) = body.find(quote) else {
            continue;
        };
        let name = &body[..close];
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '\\')
        {
            continue;
        }
        let name_at = src.len() - body.len();
        let value = body[close + 1..]
            .trim_start()
            .strip_prefix(',')
            .and_then(|v| v.split(')').next())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty() && v.len() < 80);
        out.push((name.to_string(), name_at, value));
    }
    out
}

fn line_col(src: &str, byte: usize) -> (u32, u32) {
    let before = &src[..byte.min(src.len())];
    let line = before.matches('\n').count() as u32;
    let col = before
        .rsplit('\n')
        .next()
        .unwrap_or("")
        .encode_utf16()
        .count() as u32;
    (line, col)
}

impl StubIndex {
    /// Index every `*.php` file below `root`.
    pub fn build(root: &Path) -> std::io::Result<Self> {
        let mut paths: Vec<(String, PathBuf)> = Vec::new();
        for entry in walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                !(e.file_type().is_dir()
                    && e.depth() > 0
                    && SKIP_DIRS.contains(&e.file_name().to_string_lossy().as_ref()))
            })
        {
            let entry = entry.map_err(std::io::Error::other)?;
            let p = entry.path();
            if entry.file_type().is_file() && p.extension().is_some_and(|x| x == "php") {
                let rel = p
                    .strip_prefix(root)
                    .unwrap_or(p)
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                paths.push((rel, p.to_path_buf()));
            }
        }
        let sources: Vec<(String, String)> = paths
            .into_par_iter()
            .filter_map(|(rel, p)| std::fs::read_to_string(p).ok().map(|s| (rel, s)))
            .collect();
        Ok(Self::from_sources(root, sources))
    }

    /// Index stub sources given as `(relative path, content)`.
    pub fn from_sources(root: &Path, sources: Vec<(String, String)>) -> Self {
        let parsed: Vec<(String, FileIndex, Vec<Define>, String)> = sources
            .into_par_iter()
            .map(|(rel, src)| {
                let file = parser::index_source(&rel, &src);
                let defs = defines(&src);
                (rel, file, defs, src)
            })
            .collect();
        let mut index = StubIndex {
            root: root.to_path_buf(),
            ..Default::default()
        };
        for (rel, file, defs, src) in parsed {
            for c in &file.classes {
                index
                    .classes
                    .entry(c.fqn.to_ascii_lowercase())
                    .or_insert_with(|| rel.clone());
            }
            for (i, f) in file.functions.iter().enumerate() {
                index
                    .functions
                    .entry(f.fqn.to_ascii_lowercase())
                    .or_insert_with(|| (rel.clone(), i));
            }
            for k in &file.constants {
                index
                    .constants
                    .entry(k.fqn.clone())
                    .or_insert_with(|| (rel.clone(), k.range, k.value.clone()));
            }
            for (name, at, value) in defs {
                let (line, column) = line_col(&src, at);
                let range = Range {
                    start: at,
                    end: at + name.len(),
                    line,
                    column,
                };
                index
                    .constants
                    .entry(name.trim_start_matches('\\').to_string())
                    .or_insert_with(|| (rel.clone(), range, value));
            }
            index.files.insert(rel, file);
        }
        index
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// The stub class `fqn` and its file.
    pub fn class(&self, fqn: &str) -> Option<(&str, &ClassLike)> {
        let key = fqn.trim_start_matches('\\').to_ascii_lowercase();
        let rel = self.classes.get(&key)?;
        let c = self
            .files
            .get(rel)?
            .classes
            .iter()
            .find(|c| c.fqn.eq_ignore_ascii_case(&key))?;
        Some((rel.as_str(), c))
    }

    /// `class` and its ancestors in the stubs, nearest first.
    fn lineage<'a>(&'a self, rel: &'a str, class: &'a ClassLike) -> Vec<(&'a str, &'a ClassLike)> {
        let mut out = vec![(rel, class)];
        let mut i = 0;
        while i < out.len() && out.len() < 64 {
            let c = out[i].1;
            for name in c.extends.iter().chain(&c.implements).chain(&c.traits) {
                if let Some(found) = self.class(name) {
                    if !out
                        .iter()
                        .any(|(_, x)| x.fqn.eq_ignore_ascii_case(&found.1.fqn))
                    {
                        out.push(found);
                    }
                }
            }
            i += 1;
        }
        out
    }

    fn class_target(&self, rel: &str, c: &ClassLike) -> StubTarget {
        StubTarget {
            path: rel.to_string(),
            line: c.name_range.line,
            column: c.name_range.column,
            name: c.name.clone(),
            extension: extension_of(rel),
            info: SymbolInfo {
                container: c.namespace.clone(),
                signature: c.signature(),
            },
        }
    }

    fn member_target(&self, rel: &str, c: &ClassLike, m: &Member) -> StubTarget {
        let name = match m.kind {
            SymbolKind::Method => format!("{}::{}()", c.name, m.name),
            SymbolKind::Property => format!("{}::${}", c.name, m.name),
            _ => format!("{}::{}", c.name, m.name),
        };
        StubTarget {
            path: rel.to_string(),
            line: m.name_range.line,
            column: m.name_range.column,
            name,
            extension: extension_of(rel),
            info: SymbolInfo {
                container: c.fqn.clone(),
                signature: m.signature(),
            },
        }
    }

    fn member(&self, classes: &[String], m: &ExternalMember) -> Option<StubTarget> {
        let kinds: &[SymbolKind] = match m.kind {
            SymbolKind::Constant | SymbolKind::EnumCase => {
                &[SymbolKind::Constant, SymbolKind::EnumCase]
            }
            k => match k {
                SymbolKind::Method => &[SymbolKind::Method],
                _ => &[SymbolKind::Property],
            },
        };
        for fqn in classes {
            let Some((rel, class)) = self.class(fqn) else {
                continue;
            };
            for (r, c) in self.lineage(rel, class) {
                if let Some(found) = c.members.iter().find(|x| {
                    kinds.contains(&x.kind)
                        && if x.kind == SymbolKind::Method {
                            x.name.eq_ignore_ascii_case(&m.name)
                        } else {
                            x.name == m.name
                        }
                }) {
                    return Some(self.member_target(r, c, found));
                }
            }
        }
        None
    }

    /// The declarations of an unresolved symbol (the first matching candidate).
    pub fn lookup(&self, e: &ExternalSymbol) -> Vec<StubTarget> {
        if let Some(m) = &e.member {
            return self.member(&e.classes, m).into_iter().collect();
        }
        for fqn in &e.classes {
            if let Some((rel, c)) = self.class(fqn) {
                return vec![self.class_target(rel, c)];
            }
        }
        for fqn in &e.functions {
            if let Some((rel, i)) = self.functions.get(&fqn.to_ascii_lowercase()) {
                let f = &self.files[rel].functions[*i];
                return vec![StubTarget {
                    path: rel.clone(),
                    line: f.name_range.line,
                    column: f.name_range.column,
                    name: format!("{}()", f.name),
                    extension: extension_of(rel),
                    info: SymbolInfo {
                        container: f.namespace.clone(),
                        signature: f.signature(),
                    },
                }];
            }
        }
        for fqn in &e.constants {
            if let Some((rel, range, value)) = self.constants.get(fqn) {
                let name = short_name(fqn).to_string();
                return vec![StubTarget {
                    path: rel.clone(),
                    line: range.line,
                    column: range.column,
                    name: name.clone(),
                    extension: extension_of(rel),
                    info: SymbolInfo {
                        container: String::new(),
                        signature: match value {
                            Some(v) => format!("const {name} = {v}"),
                            None => format!("const {name}"),
                        },
                    },
                }];
            }
        }
        vec![]
    }

    /// Go to Declaration inside a stub file (`src` is the content of `rel`): the names it
    /// mentions are PHP's own, so they resolve in the stubs. Returns the UTF-8 byte range
    /// of the reference and its targets.
    pub fn definition_in(
        &self,
        rel: &str,
        src: &str,
        offset: usize,
    ) -> Option<((usize, usize), Vec<StubTarget>)> {
        let file = self
            .files
            .get(rel)
            .cloned()
            .unwrap_or_else(|| parser::index_source(rel, src));
        let (start, end) = word_at(src, offset)?;
        let word = &src[start..end];
        let before = src[..start].trim_end();
        let after = src[end..].trim_start();
        let enclosing = file.class_at(start);
        let symbol = if let Some(b) = before
            .strip_suffix("->")
            .or_else(|| before.strip_suffix("::"))
        {
            let recv_start = b
                .char_indices()
                .rev()
                .take_while(|(_, c)| c.is_alphanumeric() || *c == '_' || *c == '\\' || *c == '$')
                .last()
                .map_or(b.len(), |(i, _)| i);
            let recv = &b[recv_start..];
            let class = match recv.to_ascii_lowercase().as_str() {
                "$this" | "self" | "static" => enclosing.map(|c| c.fqn.clone())?,
                "parent" => enclosing?.extends.first()?.clone(),
                r if r.starts_with('$') => return None,
                _ => file.resolve_class(recv, recv_start),
            };
            let kind = if after.starts_with('(') {
                SymbolKind::Method
            } else if word.starts_with('$') || before.ends_with("->") {
                SymbolKind::Property
            } else {
                SymbolKind::Constant
            };
            ExternalSymbol {
                classes: vec![class],
                member: Some(ExternalMember {
                    kind,
                    name: word.trim_start_matches('$').to_string(),
                }),
                ..Default::default()
            }
        } else if word.starts_with('$') {
            return None;
        } else {
            let bare = word.trim_start_matches('\\').to_string();
            if after.starts_with('(') {
                ExternalSymbol {
                    functions: vec![bare],
                    ..Default::default()
                }
            } else {
                ExternalSymbol {
                    classes: vec![file.resolve_class(word, start)],
                    constants: vec![bare],
                    ..Default::default()
                }
            }
        };
        let targets: Vec<StubTarget> = self
            .lookup(&symbol)
            .into_iter()
            // The declaration under the caret itself is no target.
            .filter(|t| !(t.path == rel && line_col(src, start).0 == t.line))
            .collect();
        Some(((start, end), targets))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORE_C: &str = "<?php\n\n/**\n * Throwable is the base interface.\n */\ninterface Throwable extends Stringable\n{\n    public function getMessage(): string;\n}\n\nclass Exception implements Throwable\n{\n    protected $message = '';\n    final public function getMessage(): string {}\n}\n\nclass RuntimeException extends Exception {}\n";
    const CORE_D: &str = "<?php\n\ndefine('E_ALL', 32767);\ndefine ( \"PHP_EOL\", \"\\n\" );\nconst PHP_VERSION_ID = 80300;\n";
    const STANDARD: &str = "<?php\n\n/**\n * Applies the callback to the elements of the given arrays\n */\nfunction array_map(?callable $callback, array $array, array ...$arrays): array {}\n";
    const STRINGABLE: &str =
        "<?php\ninterface Stringable\n{\n    public function __toString(): string;\n}\n";

    fn index() -> StubIndex {
        StubIndex::from_sources(
            Path::new("/stubs"),
            vec![
                ("Core/Core_c.php".into(), CORE_C.into()),
                ("Core/Core_d.php".into(), CORE_D.into()),
                ("standard/standard_9.php".into(), STANDARD.into()),
                ("Core/Stringable.php".into(), STRINGABLE.into()),
            ],
        )
    }

    fn classes(names: &[&str]) -> ExternalSymbol {
        ExternalSymbol {
            classes: names.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn finds_classes_functions_and_constants() {
        let idx = index();
        let t = idx.lookup(&classes(&["throwable"]));
        assert_eq!(t.len(), 1);
        assert_eq!(
            (
                t[0].path.as_str(),
                t[0].line,
                t[0].column,
                t[0].extension.as_str()
            ),
            ("Core/Core_c.php", 5, 10, "Core")
        );
        assert_eq!(
            t[0].info.signature,
            "interface Throwable extends Stringable"
        );

        let t = idx.lookup(&ExternalSymbol {
            functions: vec!["App\\array_map".into(), "array_map".into()],
            ..Default::default()
        });
        assert_eq!(t[0].name, "array_map()");
        assert_eq!(t[0].extension, "standard");
        assert!(t[0]
            .info
            .signature
            .starts_with("function array_map(?callable $callback"));

        let t = idx.lookup(&ExternalSymbol {
            constants: vec!["App\\PHP_EOL".into(), "PHP_EOL".into()],
            ..Default::default()
        });
        assert_eq!((t[0].line, t[0].column), (3, 10));
        assert_eq!(t[0].info.signature, "const PHP_EOL = \"\\n\"");
        let t = idx.lookup(&ExternalSymbol {
            constants: vec!["PHP_VERSION_ID".into()],
            ..Default::default()
        });
        assert_eq!(t[0].line, 4);
        assert!(idx
            .lookup(&ExternalSymbol {
                constants: vec!["php_eol".into()],
                ..Default::default()
            })
            .is_empty());
    }

    #[test]
    fn members_are_found_through_stub_ancestors() {
        let idx = index();
        let get_message = ExternalSymbol {
            classes: vec!["RuntimeException".into()],
            member: Some(ExternalMember {
                kind: SymbolKind::Method,
                name: "getMessage".into(),
            }),
            ..Default::default()
        };
        let t = idx.lookup(&get_message);
        assert_eq!(t[0].name, "Exception::getMessage()");
        assert_eq!(t[0].line, 13);
        assert_eq!(t[0].info.container, "Exception");
        // Interfaces' ancestors too: Throwable -> Stringable.
        let t = idx.lookup(&ExternalSymbol {
            classes: vec!["Throwable".into()],
            member: Some(ExternalMember {
                kind: SymbolKind::Method,
                name: "__toString".into(),
            }),
            ..Default::default()
        });
        assert_eq!(t[0].name, "Stringable::__toString()");
    }

    #[test]
    fn navigates_inside_stub_files() {
        let idx = index();
        let at = CORE_C.find("Throwable\n{\n    protected").unwrap() + 2;
        let ((s, e), t) = idx.definition_in("Core/Core_c.php", CORE_C, at).unwrap();
        assert_eq!(&CORE_C[s..e], "Throwable");
        assert_eq!(t[0].line, 5);
        // On the declaration itself: nothing to go to.
        let at = CORE_C.find("interface Throwable").unwrap() + 12;
        assert!(idx
            .definition_in("Core/Core_c.php", CORE_C, at)
            .unwrap()
            .1
            .is_empty());
    }

    #[test]
    fn builds_from_a_folder_and_skips_tests() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Core")).unwrap();
        std::fs::create_dir_all(dir.path().join("tests")).unwrap();
        std::fs::write(dir.path().join("Core/Core_c.php"), CORE_C).unwrap();
        std::fs::write(
            dir.path().join("tests/Throwable.php"),
            "<?php class Nope {}",
        )
        .unwrap();
        let idx = StubIndex::build(dir.path()).unwrap();
        assert_eq!(idx.file_count(), 1);
        assert!(idx.class("Exception").is_some());
        assert!(idx.class("Nope").is_none());
    }
}
