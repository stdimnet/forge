use std::collections::HashMap;
use std::sync::OnceLock;

use nucleo_matcher::{Config, Matcher, Utf32Str};
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use regex::bytes::Regex;
use serde::{Deserialize, Serialize};

use crate::index::FileRec;

const MAX_SYMBOL_FILE_BYTES: usize = 1024 * 1024;
const MAX_SYMBOLS_PER_FILE: usize = 5000;
const MAX_NAME_BYTES: usize = 120;
const SCORE_CHUNK: usize = 1 << 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Class,
    Interface,
    Trait,
    Enum,
    Struct,
    Module,
    TypeAlias,
    Function,
    Method,
    Constant,
    Property,
}

impl Kind {
    fn is_type(self) -> bool {
        matches!(
            self,
            Kind::Class
                | Kind::Interface
                | Kind::Trait
                | Kind::Enum
                | Kind::Struct
                | Kind::Module
                | Kind::TypeAlias
        )
    }

    /// Name of the matching `SearchItemKind` in the UI.
    fn ui_name(self) -> &'static str {
        match self {
            Kind::Class | Kind::Struct | Kind::Module | Kind::TypeAlias => "class",
            Kind::Interface => "interface",
            Kind::Trait => "trait",
            Kind::Enum => "enum",
            Kind::Function => "function",
            Kind::Method => "method",
            Kind::Constant => "constant",
            Kind::Property => "property",
        }
    }

    fn from_keyword(word: &[u8]) -> Option<Kind> {
        Some(match word {
            b"class" | b"object" | b"record" => Kind::Class,
            b"interface" | b"protocol" => Kind::Interface,
            b"trait" => Kind::Trait,
            b"enum" => Kind::Enum,
            b"struct" | b"union" => Kind::Struct,
            b"mod" | b"module" | b"namespace" => Kind::Module,
            b"type" => Kind::TypeAlias,
            b"fn" | b"function" | b"def" | b"func" | b"fun" => Kind::Function,
            b"const" | b"static" => Kind::Constant,
            _ => return None,
        })
    }
}

pub struct RawSymbol {
    pub name: String,
    pub kind: Kind,
    pub line: u32,
    /// Index of the enclosing type in the same file's symbol list.
    pub container: Option<usize>,
}

struct Rule {
    re: Regex,
    /// Used when the pattern has no `kw` group.
    kind: Kind,
}

fn rule(pattern: &str, kind: Kind) -> Rule {
    Rule {
        re: Regex::new(pattern).expect("built-in symbol pattern"),
        kind,
    }
}

const TYPE_KEYWORDS: &str =
    "class|interface|trait|enum|struct|union|object|record|protocol|module|namespace";

fn languages() -> &'static HashMap<&'static str, Vec<Rule>> {
    static RULES: OnceLock<HashMap<&'static str, Vec<Rule>>> = OnceLock::new();
    RULES.get_or_init(|| {
        let mut map: HashMap<&'static str, Vec<Rule>> = HashMap::new();
        let mut add = |exts: &[&'static str], rules: Vec<Rule>| {
            for ext in exts {
                map.insert(ext, rules.iter().map(|r| Rule { re: r.re.clone(), kind: r.kind }).collect());
            }
        };

        add(
            &["php", "phtml", "inc"],
            vec![
                rule(r"(?m)^(?P<ind>[ \t]*)(?:(?:abstract|final|readonly)[ \t]+)*(?P<kw>class|interface|trait|enum)[ \t]+(?P<name>\w+)", Kind::Class),
                rule(r"(?m)^(?P<ind>[ \t]*)(?:(?:public|protected|private|static|final|abstract)[ \t]+)*function[ \t]+&?(?P<name>\w+)", Kind::Function),
                rule(r"(?m)^(?P<ind>[ \t]*)(?:(?:public|protected|private|final)[ \t]+)*const[ \t]+(?:\w+[ \t]+)?(?P<name>\w+)[ \t]*=", Kind::Constant),
                rule(r"(?m)^(?P<ind>[ \t]*)case[ \t]+(?P<name>\w+)[ \t]*(?:=|;)", Kind::Constant),
                rule(r"(?m)^(?P<ind>[ \t]*)(?:(?:public|protected|private|static|readonly)[ \t]+)+(?:[\w?\\|]+[ \t]+)?(?P<name>\$\w+)", Kind::Property),
            ],
        );
        add(
            &["rs"],
            vec![
                rule(r"(?m)^(?P<ind>[ \t]*)(?:pub(?:\([^)]*\))?[ \t]+)?(?:(?:unsafe|async|const|extern(?:[ \t]+[^ \t]+)?)[ \t]+)*(?P<kw>fn)[ \t]+(?P<name>\w+)", Kind::Function),
                rule(r"(?m)^(?P<ind>[ \t]*)(?:pub(?:\([^)]*\))?[ \t]+)?(?P<kw>struct|enum|trait|union|type|mod|static)[ \t]+(?P<name>\w+)", Kind::Struct),
                rule(r"(?m)^(?P<ind>[ \t]*)(?:pub(?:\([^)]*\))?[ \t]+)?(?P<kw>const)[ \t]+(?P<name>[A-Z_][A-Z_0-9]*)\b", Kind::Constant),
            ],
        );
        add(
            &["js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts"],
            vec![
                rule(r"(?m)^(?P<ind>[ \t]*)(?:export[ \t]+)?(?:default[ \t]+)?(?:declare[ \t]+)?(?:abstract[ \t]+)?(?P<kw>class|interface|enum|type|namespace)[ \t]+(?P<name>\w+)", Kind::Class),
                rule(r"(?m)^(?P<ind>[ \t]*)(?:export[ \t]+)?(?:default[ \t]+)?(?:async[ \t]+)?function[ \t]*\*?[ \t]*(?P<name>\w+)", Kind::Function),
                rule(r"(?m)^(?P<ind>[ \t]*)(?:export[ \t]+)?(?:const|let|var)[ \t]+(?P<name>\w+)[ \t]*(?::[^=\n]+)?=[ \t]*(?:async[ \t]*)?(?:\([^)\n]*\)|\w+)[ \t]*(?::[^=\n]+)?=>", Kind::Function),
            ],
        );
        add(
            &["py", "pyi"],
            vec![
                rule(r"(?m)^(?P<ind>[ \t]*)(?P<kw>class)[ \t]+(?P<name>\w+)", Kind::Class),
                rule(r"(?m)^(?P<ind>[ \t]*)(?:async[ \t]+)?(?P<kw>def)[ \t]+(?P<name>\w+)", Kind::Function),
            ],
        );
        add(
            &["go"],
            vec![
                rule(r"(?m)^func[ \t]+\([^)]*\)[ \t]*(?P<name>\w+)", Kind::Method),
                rule(r"(?m)^func[ \t]+(?P<name>\w+)", Kind::Function),
                rule(r"(?m)^type[ \t]+(?P<name>\w+)[ \t]+(?P<kw>struct|interface)", Kind::Struct),
            ],
        );
        let jvm_like = format!(
            r"(?m)^(?P<ind>[ \t]*)(?:(?:public|protected|private|internal|static|final|abstract|sealed|open|data|partial|override|export)[ \t]+)*(?P<kw>{TYPE_KEYWORDS}|record)[ \t]+(?P<name>\w+)"
        );
        add(
            &["java", "kt", "kts", "cs", "scala", "swift", "dart"],
            vec![
                rule(&jvm_like, Kind::Class),
                rule(r"(?m)^(?P<ind>[ \t]*)(?:(?:public|protected|private|internal|override|open|suspend|inline|static|final|abstract)[ \t]+)*(?P<kw>fun|def|func)[ \t]+(?:<[^>]*>[ \t]*)?(?:[\w.]+\.)?(?P<name>\w+)", Kind::Function),
            ],
        );
        add(
            &["c", "h", "cpp", "cc", "cxx", "hpp", "hh"],
            vec![rule(r"(?m)^(?P<ind>[ \t]*)(?:typedef[ \t]+)?(?P<kw>class|struct|enum|union|namespace)[ \t]+(?:class[ \t]+)?(?P<name>\w+)[ \t]*(?:[{:]|$)", Kind::Class)],
        );
        add(
            &["rb"],
            vec![
                rule(r"(?m)^(?P<ind>[ \t]*)(?P<kw>class|module)[ \t]+(?P<name>[A-Z]\w*)", Kind::Class),
                rule(r"(?m)^(?P<ind>[ \t]*)(?P<kw>def)[ \t]+(?:self\.)?(?P<name>\w+)", Kind::Function),
            ],
        );
        add(&["lua"], vec![rule(r"(?m)^(?P<ind>[ \t]*)(?:local[ \t]+)?function[ \t]+(?P<name>[\w.:]+)", Kind::Function)]);
        map
    })
}

/// Declarations of a source file, found with line patterns (a light ctags, no parsing).
pub(crate) fn extract(path: &str, bytes: &[u8]) -> Vec<RawSymbol> {
    if bytes.len() > MAX_SYMBOL_FILE_BYTES {
        return Vec::new();
    }
    let Some(ext) = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()) else {
        return Vec::new();
    };
    let Some(rules) = languages().get(ext.as_str()) else {
        return Vec::new();
    };

    struct Hit {
        at: usize,
        indent: usize,
        name: String,
        kind: Kind,
    }
    let mut hits: Vec<Hit> = Vec::new();
    for r in rules {
        for caps in r.re.captures_iter(bytes) {
            let Some(name) = caps.name("name") else {
                continue;
            };
            if name.as_bytes().len() > MAX_NAME_BYTES {
                continue;
            }
            let kind = caps
                .name("kw")
                .and_then(|k| Kind::from_keyword(k.as_bytes()))
                .unwrap_or(r.kind);
            hits.push(Hit {
                at: name.start(),
                indent: caps.name("ind").map_or(0, |i| i.as_bytes().len()),
                name: String::from_utf8_lossy(name.as_bytes()).into_owned(),
                kind,
            });
        }
    }
    hits.sort_by_key(|h| h.at);
    hits.truncate(MAX_SYMBOLS_PER_FILE);

    let mut out: Vec<RawSymbol> = Vec::with_capacity(hits.len());
    let mut types: Vec<(usize, usize)> = Vec::new();
    let (mut line, mut counted) = (1u32, 0usize);
    for h in hits {
        line += bytes[counted..h.at].iter().filter(|&&b| b == b'\n').count() as u32;
        counted = h.at;
        while types.last().is_some_and(|&(indent, _)| indent >= h.indent) {
            types.pop();
        }
        let container = types.last().map(|&(_, idx)| idx);
        let kind = if h.kind == Kind::Function && container.is_some() {
            Kind::Method
        } else {
            h.kind
        };
        if kind.is_type() {
            types.push((h.indent, out.len()));
        }
        out.push(RawSymbol {
            name: h.name,
            kind,
            line,
            container,
        });
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SymRec {
    file: u32,
    line: u32,
    name_off: u32,
    name_len: u16,
    cont_off: u32,
    cont_len: u16,
    kind: Kind,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct SymbolTable {
    names: String,
    recs: Vec<SymRec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolHit {
    pub name: String,
    /// UI kind: class, interface, trait, enum, function, method, property or constant.
    pub kind: String,
    pub path: String,
    pub line: u32,
    pub container: Option<String>,
    pub score: u32,
    /// Character indices of `name` that matched the query.
    pub matched: Vec<u32>,
}

fn has_extension(path: &str, extensions: &[String]) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, ext)| extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
}

impl SymbolTable {
    pub fn add_file(&mut self, file: u32, symbols: &[RawSymbol]) {
        let mut spans: Vec<(u32, u16)> = Vec::with_capacity(symbols.len());
        for s in symbols {
            let off = self.names.len() as u32;
            self.names.push_str(&s.name);
            let len = s.name.len() as u16;
            spans.push((off, len));
            let (cont_off, cont_len) = s.container.map_or((0, 0), |c| spans[c]);
            self.recs.push(SymRec {
                file,
                line: s.line,
                name_off: off,
                name_len: len,
                cont_off,
                cont_len,
                kind: s.kind,
            });
        }
    }

    fn name(&self, r: &SymRec) -> &str {
        &self.names[r.name_off as usize..r.name_off as usize + r.name_len as usize]
    }

    pub fn search(
        &self,
        files: &[Option<FileRec>],
        query: &str,
        types_only: bool,
        limit: usize,
        skip_extensions: &[String],
        exact: bool,
    ) -> Vec<SymbolHit> {
        let query = query.trim();
        if query.is_empty() || limit == 0 {
            return Vec::new();
        }
        let pattern = crate::index::pattern(query, exact);
        let mut scored: Vec<(u32, usize)> = par_chunks!(self.recs, SCORE_CHUNK)
            .enumerate()
            .flat_map(|(chunk_no, chunk)| {
                let mut matcher = Matcher::new(Config::DEFAULT);
                let mut buf = Vec::new();
                let mut out = Vec::new();
                for (i, r) in chunk.iter().enumerate() {
                    if (types_only && !r.kind.is_type()) || files[r.file as usize].is_none() {
                        continue;
                    }
                    let Some(score) =
                        pattern.score(Utf32Str::new(self.name(r), &mut buf), &mut matcher)
                    else {
                        continue;
                    };
                    if !skip_extensions.is_empty()
                        && files[r.file as usize]
                            .as_ref()
                            .is_some_and(|f| has_extension(&f.path, skip_extensions))
                    {
                        continue;
                    }
                    out.push((score, chunk_no * SCORE_CHUNK + i));
                }
                out
            })
            .collect();
        scored.sort_unstable_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| {
                    self.name(&self.recs[a.1])
                        .len()
                        .cmp(&self.name(&self.recs[b.1]).len())
                })
                .then(a.1.cmp(&b.1))
        });
        scored.truncate(limit);

        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut buf = Vec::new();
        scored
            .into_iter()
            .map(|(score, idx)| {
                let r = &self.recs[idx];
                let name = self.name(r);
                let mut matched = Vec::new();
                pattern.indices(Utf32Str::new(name, &mut buf), &mut matcher, &mut matched);
                matched.sort_unstable();
                matched.dedup();
                let container = (r.cont_len > 0).then(|| {
                    self.names[r.cont_off as usize..r.cont_off as usize + r.cont_len as usize]
                        .to_string()
                });
                SymbolHit {
                    name: name.to_string(),
                    kind: r.kind.ui_name().to_string(),
                    path: files[r.file as usize]
                        .as_ref()
                        .map(|f| f.path.clone())
                        .unwrap_or_default(),
                    line: r.line,
                    container,
                    score,
                    matched,
                }
            })
            .collect()
    }
}
