use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{Error, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use regex::bytes::{Regex, RegexBuilder};
use regex_syntax::hir::literal::{ExtractKind, Extractor};
use regex_syntax::hir::Hir;
use serde::{Deserialize, Serialize};

use crate::index::Index;

const WINDOW_CHARS: usize = 400;
const LEAD_CHARS: usize = 100;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Query {
    pub pattern: String,
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_words: bool,
    /// Comma or space separated globs like `*.php, !*.min.js`; patterns without `/` match at any depth.
    pub file_mask: String,
    /// Workspace-relative directory to restrict the search to.
    pub dir: Option<String>,
    /// 0 means unlimited.
    pub max_matches: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    /// 1-based line number.
    pub line: u32,
    /// 0-based character column of the match in the full line.
    pub column: u32,
    /// The line, or a window of it around the match when the line is very long.
    pub text: String,
    /// Highlight range inside `text`, in characters.
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMatches {
    pub path: String,
    pub matches: Vec<Match>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub files: Vec<FileMatches>,
    pub total_matches: usize,
    pub truncated: bool,
    pub cancelled: bool,
    pub searched_files: usize,
    /// True while the index is still being built or refreshed, so results may be incomplete.
    pub indexing: bool,
}

pub struct Compiled {
    re: Regex,
    literals: Vec<Vec<Vec<u8>>>,
    include: Option<GlobSet>,
    exclude: Option<GlobSet>,
    dir: Option<String>,
    max: usize,
}

impl Query {
    pub fn compile(&self) -> Result<Compiled> {
        if self.pattern.is_empty() {
            return Err(Error("empty search pattern".into()));
        }
        let mut pat = if self.regex {
            self.pattern.clone()
        } else {
            regex::escape(&self.pattern)
        };
        if self.whole_words {
            pat = format!(r"\b(?:{pat})\b");
        }
        let re = RegexBuilder::new(&pat)
            .case_insensitive(!self.case_sensitive)
            .multi_line(true)
            .build()
            .map_err(|e| Error(format!("invalid pattern: {e}")))?;

        let literals = regex_syntax::ParserBuilder::new()
            .case_insensitive(!self.case_sensitive)
            .multi_line(true)
            .build()
            .parse(&pat)
            .map(|hir| {
                [ExtractKind::Prefix, ExtractKind::Suffix]
                    .into_iter()
                    .filter_map(|k| required_literals(&hir, k))
                    .collect()
            })
            .unwrap_or_default();

        let (include, exclude) = parse_mask(&self.file_mask)?;
        Ok(Compiled {
            re,
            literals,
            include,
            exclude,
            dir: self
                .dir
                .as_deref()
                .map(|d| d.trim_matches('/').to_string())
                .filter(|d| !d.is_empty()),
            max: if self.max_matches == 0 {
                usize::MAX
            } else {
                self.max_matches
            },
        })
    }
}

/// Literals of which at least one occurs in every match, or `None` when no useful bound exists.
fn required_literals(hir: &Hir, kind: ExtractKind) -> Option<Vec<Vec<u8>>> {
    let mut extractor = Extractor::new();
    extractor.kind(kind);
    let seq = extractor.extract(hir);
    let mut out = Vec::new();
    for lit in seq.literals()? {
        if lit.as_bytes().len() < 3 {
            return None;
        }
        out.push(lit.as_bytes().to_ascii_lowercase());
    }
    out.sort();
    out.dedup();
    Some(out)
}

fn parse_mask(mask: &str) -> Result<(Option<GlobSet>, Option<GlobSet>)> {
    let (mut inc, mut exc) = (GlobSetBuilder::new(), GlobSetBuilder::new());
    let (mut has_inc, mut has_exc) = (false, false);
    for token in mask
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|t| !t.is_empty())
    {
        let (negated, glob) = match token.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, token),
        };
        let glob = if glob.contains('/') {
            glob.to_string()
        } else {
            format!("**/{glob}")
        };
        let glob =
            Glob::new(&glob).map_err(|e| Error(format!("invalid file mask `{token}`: {e}")))?;
        if negated {
            exc.add(glob);
            has_exc = true;
        } else {
            inc.add(glob);
            has_inc = true;
        }
    }
    let build = |b: GlobSetBuilder| {
        b.build()
            .map_err(|e| Error(format!("invalid file mask: {e}")))
    };
    Ok((
        if has_inc { Some(build(inc)?) } else { None },
        if has_exc { Some(build(exc)?) } else { None },
    ))
}

impl Compiled {
    fn accepts(&self, path: &str) -> bool {
        if let Some(dir) = &self.dir {
            if !(path.len() > dir.len()
                && path.starts_with(dir.as_str())
                && path.as_bytes()[dir.len()] == b'/')
            {
                return false;
            }
        }
        let included = match &self.include {
            Some(set) => set.is_match(path),
            None => true,
        };
        included && !self.exclude.as_ref().is_some_and(|s| s.is_match(path))
    }
}

/// Run a compiled query. `read` returns a file's bytes by path; `cancelled` is polled between files.
pub fn run(
    index: &Index,
    q: &Compiled,
    read: impl Fn(&str) -> Option<Vec<u8>> + Sync,
    cancelled: impl Fn() -> bool + Sync,
) -> Outcome {
    let ids = index.candidates(&q.literals);
    let found = AtomicUsize::new(0);
    let searched = AtomicUsize::new(0);

    let mut files: Vec<FileMatches> = par_iter!(ids)
        .filter_map(|&id| {
            if cancelled() || found.load(Ordering::Relaxed) >= q.max {
                return None;
            }
            let rec = index.rec(id)?;
            if !q.accepts(&rec.path) {
                return None;
            }
            let bytes = read(&rec.path)?;
            searched.fetch_add(1, Ordering::Relaxed);
            let matches = find_matches(
                &q.re,
                &bytes,
                q.max.saturating_sub(found.load(Ordering::Relaxed)),
            );
            if matches.is_empty() {
                return None;
            }
            found.fetch_add(matches.len(), Ordering::Relaxed);
            Some(FileMatches {
                path: rec.path.clone(),
                matches,
            })
        })
        .collect();

    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut remaining = q.max;
    files.retain_mut(|f| {
        if remaining == 0 {
            return false;
        }
        f.matches.truncate(remaining);
        remaining -= f.matches.len();
        true
    });
    let total_matches = files.iter().map(|f| f.matches.len()).sum();
    Outcome {
        files,
        total_matches,
        truncated: found.load(Ordering::Relaxed) >= q.max,
        cancelled: cancelled(),
        searched_files: searched.load(Ordering::Relaxed),
        indexing: false,
    }
}

fn find_matches(re: &Regex, hay: &[u8], limit: usize) -> Vec<Match> {
    let mut out = Vec::new();
    if limit == 0 {
        return out;
    }
    let mut line_starts: Vec<usize> = Vec::new();
    for m in re.find_iter(hay) {
        if m.is_empty() {
            continue;
        }
        if line_starts.is_empty() {
            line_starts.push(0);
            line_starts.extend(
                hay.iter()
                    .enumerate()
                    .filter(|(_, &b)| b == b'\n')
                    .map(|(i, _)| i + 1),
            );
        }
        out.push(to_match(hay, &line_starts, m.start(), m.end()));
        if out.len() >= limit {
            break;
        }
    }
    out
}

fn to_match(hay: &[u8], line_starts: &[usize], start: usize, end: usize) -> Match {
    let line = line_starts.partition_point(|&s| s <= start) - 1;
    let line_start = line_starts[line];
    let mut line_end = line_starts.get(line + 1).map_or(hay.len(), |&n| n - 1);
    if line_end > line_start && hay[line_end - 1] == b'\r' {
        line_end -= 1;
    }
    let chars = |a: usize, b: usize| String::from_utf8_lossy(&hay[a..b]).chars().count();
    let col = chars(line_start, start);
    let col_end = col + chars(start, end.min(line_end).max(start));
    let text: Vec<char> = String::from_utf8_lossy(&hay[line_start..line_end])
        .chars()
        .collect();
    let window_start = if text.len() > WINDOW_CHARS {
        col.saturating_sub(LEAD_CHARS)
    } else {
        0
    };
    let window_end = (window_start + WINDOW_CHARS).min(text.len());
    Match {
        line: line as u32 + 1,
        column: col as u32,
        text: text[window_start..window_end].iter().collect(),
        start: (col - window_start) as u32,
        end: (col_end.min(window_end) - window_start) as u32,
    }
}
