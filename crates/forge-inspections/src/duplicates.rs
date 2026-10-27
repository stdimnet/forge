//! The duplicate code detector (PHPStorm's "Duplicated code fragment" and Code | Analyze
//! Code | Locate Duplicates): files are split into tokens (comments and formatting do not
//! count), windows of the minimum size are hashed, and equal windows are extended to the
//! longest common fragment. With "anonymize identifiers" names of variables, functions and
//! fields do not matter; with "anonymize literals" string and number values do not either.

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::lang::{self, Kind};
use crate::problem::{Location, Problem};
use crate::registry::DUPLICATED_CODE;
use crate::text::LineIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DuplicateOptions {
    pub min_tokens: usize,
    pub anonymize_identifiers: bool,
    pub anonymize_literals: bool,
}

impl Default for DuplicateOptions {
    fn default() -> Self {
        DuplicateOptions {
            min_tokens: 60,
            anonymize_identifiers: true,
            anonymize_literals: false,
        }
    }
}

impl DuplicateOptions {
    /// From the inspection's option values.
    pub fn from_options(options: &std::collections::BTreeMap<String, serde_json::Value>) -> Self {
        let d = DuplicateOptions::default();
        DuplicateOptions {
            min_tokens: options
                .get("minTokens")
                .and_then(|v| v.as_u64())
                .map_or(d.min_tokens, |n| n as usize)
                .max(5),
            anonymize_identifiers: options
                .get("anonymizeIdentifiers")
                .and_then(|v| v.as_bool())
                .unwrap_or(d.anonymize_identifiers),
            anonymize_literals: options
                .get("anonymizeLiterals")
                .and_then(|v| v.as_bool())
                .unwrap_or(d.anonymize_literals),
        }
    }
}

/// One copy of a duplicated fragment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fragment {
    pub path: String,
    /// Bytes.
    pub start: usize,
    pub end: usize,
    /// 0-based lines and UTF-16 columns.
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

impl Fragment {
    pub fn location(&self) -> Location {
        Location {
            path: self.path.clone(),
            line: self.line,
            column: self.column,
            end_line: self.end_line,
            end_column: self.end_column,
        }
    }
}

/// Fragments with the same code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub tokens: usize,
    /// Lines of the first fragment.
    pub lines: usize,
    pub fragments: Vec<Fragment>,
}

/// Two copies (file, token index) and the common length.
type Match = ((u32, u32), (u32, u32), usize);

#[derive(Debug, Clone, Copy)]
struct Tok {
    hash: u64,
    start: usize,
    end: usize,
}

const KEYWORDS: &[&str] = &[
    "abstract",
    "and",
    "as",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "def",
    "default",
    "defer",
    "del",
    "do",
    "echo",
    "elif",
    "else",
    "elseif",
    "enum",
    "except",
    "extends",
    "false",
    "final",
    "finally",
    "fn",
    "for",
    "foreach",
    "from",
    "fun",
    "func",
    "function",
    "global",
    "go",
    "if",
    "impl",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "is",
    "isset",
    "lambda",
    "let",
    "loop",
    "match",
    "mut",
    "namespace",
    "new",
    "nil",
    "none",
    "not",
    "null",
    "or",
    "package",
    "pass",
    "print",
    "private",
    "protected",
    "pub",
    "public",
    "raise",
    "readonly",
    "return",
    "self",
    "static",
    "struct",
    "super",
    "switch",
    "this",
    "throw",
    "throws",
    "trait",
    "true",
    "try",
    "type",
    "typeof",
    "unset",
    "use",
    "val",
    "var",
    "void",
    "when",
    "where",
    "while",
    "with",
    "yield",
    "None",
    "True",
    "False",
    "Self",
    "int",
    "string",
    "bool",
    "float",
    "array",
    "mixed",
    "object",
];

fn fnv(s: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c >= 0x80
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}

/// The tokens of a file (code only).
fn tokenize(path: &str, text: &str, opts: &DuplicateOptions) -> Vec<Tok> {
    let lang = lang::for_path(path);
    let lexed = lang::lex(text, &lang);
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match lexed.kinds[i] {
            Kind::Comment | Kind::Outside => {
                i += 1;
                continue;
            }
            Kind::String => {
                let start = i;
                while i < b.len() && lexed.kinds[i] == Kind::String {
                    i += 1;
                }
                let hash = if opts.anonymize_literals {
                    fnv(b"\0str")
                } else {
                    fnv(&b[start..i])
                };
                out.push(Tok {
                    hash,
                    start,
                    end: i,
                });
                continue;
            }
            Kind::Code => {}
        }
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if c == b'$' && b.get(i + 1).is_some_and(|&n| is_ident_start(n)) || is_ident_start(c) {
            i += 1;
            while i < b.len() && is_ident(b[i]) && lexed.kinds[i] == Kind::Code {
                i += 1;
            }
            let word = &text[start..i];
            let keyword = KEYWORDS.contains(&word.trim_start_matches('$')) || word == "$this";
            let hash = if opts.anonymize_identifiers && !keyword {
                fnv(if c == b'$' { b"\0$id" } else { b"\0id" })
            } else {
                fnv(word.as_bytes())
            };
            out.push(Tok {
                hash,
                start,
                end: i,
            });
        } else if c.is_ascii_digit() {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') {
                i += 1;
            }
            let hash = if opts.anonymize_literals {
                fnv(b"\0num")
            } else {
                fnv(&b[start..i])
            };
            out.push(Tok {
                hash,
                start,
                end: i,
            });
        } else {
            let len = text[i..].chars().next().map_or(1, char::len_utf8);
            i += len;
            out.push(Tok {
                hash: fnv(&b[start..i]),
                start,
                end: i,
            });
        }
    }
    out
}

/// Duplicates among `files` (`(path, text)`); `cancelled` is polled now and then.
pub fn find(
    files: &[(&str, &str)],
    opts: &DuplicateOptions,
    cancelled: &dyn Fn() -> bool,
) -> Vec<DuplicateGroup> {
    let w = opts.min_tokens.max(5);
    let toks: Vec<Vec<Tok>> = files.iter().map(|(p, t)| tokenize(p, t, opts)).collect();
    // Rolling hashes of every window of `w` tokens.
    const BASE: u64 = 1_000_003;
    let pow = (1..w).fold(1u64, |acc, _| acc.wrapping_mul(BASE));
    let mut buckets: HashMap<u64, Vec<(u32, u32)>> = HashMap::new();
    for (f, ts) in toks.iter().enumerate() {
        if ts.len() < w {
            continue;
        }
        let mut h: u64 = 0;
        for t in &ts[..w] {
            h = h.wrapping_mul(BASE).wrapping_add(t.hash);
        }
        buckets.entry(h).or_default().push((f as u32, 0));
        for i in w..ts.len() {
            h = h
                .wrapping_sub(ts[i - w].hash.wrapping_mul(pow))
                .wrapping_mul(BASE)
                .wrapping_add(ts[i].hash);
            buckets
                .entry(h)
                .or_default()
                .push((f as u32, (i + 1 - w) as u32));
        }
    }
    let tok = |f: u32, i: usize| toks[f as usize].get(i).map(|t| t.hash);
    // Maximal matches of two copies.
    let mut matches: Vec<Match> = Vec::new();
    let mut polled = 0usize;
    for list in buckets.values() {
        if list.len() < 2 || list.len() > 64 {
            continue;
        }
        polled += 1;
        if polled.is_multiple_of(256) && cancelled() {
            return Vec::new();
        }
        for (x, &(fa, ia)) in list.iter().enumerate() {
            for &(fb, ib) in &list[x + 1..] {
                let (ia, ib) = (ia as usize, ib as usize);
                if fa == fb && ia + w > ib {
                    continue;
                }
                if (0..w).any(|k| tok(fa, ia + k) != tok(fb, ib + k)) {
                    continue;
                }
                // Not the start of the common fragment: an earlier window covers it.
                if ia > 0
                    && ib > 0
                    && tok(fa, ia - 1) == tok(fb, ib - 1)
                    && !(fa == fb && ia + w > ib - 1)
                {
                    continue;
                }
                let mut len = w;
                while tok(fa, ia + len).is_some()
                    && tok(fa, ia + len) == tok(fb, ib + len)
                    && !(fa == fb && ia + len >= ib)
                {
                    len += 1;
                }
                matches.push(((fa, ia as u32), (fb, ib as u32), len));
            }
        }
    }
    // Copies that share a start and have about the same length form one group (three
    // copies match pairwise with slightly different lengths); its length is the shortest.
    matches.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)).then(a.1.cmp(&b.1)));
    let mut kept: Vec<(usize, BTreeSet<(u32, u32)>)> = Vec::new();
    for (a, b, len) in matches {
        let near = kept
            .iter_mut()
            .find(|(l, set)| len * 5 >= *l * 4 && (set.contains(&a) || set.contains(&b)));
        match near {
            Some((l, set)) => {
                *l = (*l).min(len);
                set.insert(a);
                set.insert(b);
            }
            None => kept.push((len, [a, b].into_iter().collect())),
        }
    }
    let indexes: Vec<LineIndex> = files.iter().map(|(_, t)| LineIndex::new(t)).collect();
    let mut out: Vec<DuplicateGroup> = kept
        .into_iter()
        .map(|(len, set)| {
            let fragments: Vec<Fragment> = set
                .into_iter()
                .map(|(f, i)| {
                    let ts = &toks[f as usize];
                    let (i, f) = (i as usize, f as usize);
                    let start = ts[i].start;
                    let end = ts[i + len - 1].end;
                    let (line, column) = indexes[f].position(start);
                    let (end_line, end_column) = indexes[f].position(end);
                    Fragment {
                        path: files[f].0.to_string(),
                        start,
                        end,
                        line,
                        column,
                        end_line,
                        end_column,
                    }
                })
                .collect();
            let lines = fragments
                .first()
                .map_or(0, |f| (f.end_line - f.line + 1) as usize);
            DuplicateGroup {
                tokens: len,
                lines,
                fragments,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.tokens
            .cmp(&a.tokens)
            .then_with(|| a.fragments[0].path.cmp(&b.fragments[0].path))
            .then_with(|| a.fragments[0].start.cmp(&b.fragments[0].start))
    });
    out.truncate(500);
    out
}

/// The `DuplicatedCode` problems of the groups: one per copy, pointing at the others.
pub fn problems(groups: &[DuplicateGroup]) -> Vec<Problem> {
    let mut out = Vec::new();
    for g in groups {
        for (k, f) in g.fragments.iter().enumerate() {
            let mut p = Problem::new(
                DUPLICATED_CODE,
                &f.path,
                f.start,
                f.end,
                format!(
                    "Duplicated code fragment ({} line{} long, {} cop{})",
                    g.lines,
                    if g.lines == 1 { "" } else { "s" },
                    g.fragments.len(),
                    if g.fragments.len() == 1 { "y" } else { "ies" }
                ),
            );
            p.related = g
                .fragments
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != k)
                .map(|(_, o)| o.location())
                .collect();
            out.push(p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "<?php\nfunction total(array $items): float\n{\n    $sum = 0;\n    foreach ($items as $item) {\n        // price times quantity\n        $sum += $item->price * $item->quantity;\n    }\n    return round($sum, 2);\n}\n";
    const B: &str = "<?php\nfunction amount(array $lines): float\n{\n    $total = 0;\n    foreach ($lines as $line) {\n        $total += $line->price * $line->quantity;\n    }\n    return round($total, 2);\n}\n";

    fn opts(min: usize, anon: bool) -> DuplicateOptions {
        DuplicateOptions {
            min_tokens: min,
            anonymize_identifiers: anon,
            anonymize_literals: false,
        }
    }

    #[test]
    fn finds_renamed_copies_when_anonymizing() {
        let files = [("a.php", A), ("b.php", B)];
        let found = find(&files, &opts(20, true), &|| false);
        assert_eq!(found.len(), 1, "{found:#?}");
        let g = &found[0];
        assert_eq!(g.fragments.len(), 2);
        assert!(A[g.fragments[0].start..g.fragments[0].end].starts_with("function total"));
        assert!(A[g.fragments[0].start..g.fragments[0].end].ends_with('}'));
        assert_eq!(g.lines, 9);
        // Without anonymizing the names differ.
        assert!(find(&files, &opts(20, false), &|| false).is_empty());
        // Too short for a large minimum.
        assert!(find(&files, &opts(200, true), &|| false).is_empty());
    }

    #[test]
    fn copies_in_one_file_and_three_copies() {
        let body =
            "    $x = compute($a, $b);\n    if ($x > 10) {\n        log_it('big', $x);\n    }\n";
        let src = format!("<?php\nfunction a() {{\n{body}}}\nfunction b() {{\n{body}}}\nfunction c() {{\n{body}}}\n");
        let files = [("x.php", src.as_str())];
        let found = find(&files, &opts(15, false), &|| false);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].fragments.len(), 3);
        let problems = problems(&found);
        assert_eq!(problems.len(), 3);
        assert_eq!(problems[0].related.len(), 2);
        assert!(problems[0].message.contains("3 copies"));
    }

    #[test]
    fn comments_and_whitespace_do_not_matter() {
        let a =
            "function f(a, b) {\n  const s = a + b; // sum\n  return s * 2 + a - b * 3 + 7;\n}\n";
        let b = "function f(a,b){const s=a+b;/* x */return s*2+a-b*3+7;}";
        let found = find(&[("a.js", a), ("b.js", b)], &opts(10, false), &|| false);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].tokens, tokenize("b.js", b, &opts(10, false)).len());
    }
}
