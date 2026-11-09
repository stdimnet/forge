//! Find in Files (PHPStorm's Ctrl+Shift+F): the search request, file masks and per-line
//! matching. The crate is pure so the desktop backend (walking the project with
//! `forge-core`), the remote agent and the browser-mode WebAssembly build share it.
//!
//! - The query is a literal or a regular expression (`regex` syntax), optionally
//!   case-sensitive and limited to whole words.
//! - The file mask is a comma-separated list of globs (`*.php, *.twig`); `!` excludes
//!   (`!*Test.php`). A glob without `/` matches the file name, otherwise the whole path.
//! - Matches are reported per line, like PHPStorm's result list: the line text (trimmed and
//!   cut around the first match) with every match range in it. Columns and ranges are UTF-16
//!   offsets so the UI can use them directly.
//!
//! [`todo`] scans comments for TODO patterns (the TODO tool window).

pub mod todo;

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

/// Files larger than this are skipped (bundles, dumps).
pub const MAX_FILE_SIZE: u64 = 4 * 1024 * 1024;
/// Default cap on reported matches.
pub const DEFAULT_LIMIT: usize = 2000;
/// Longest line preview sent to the UI.
const PREVIEW_CHARS: usize = 220;

fn default_limit() -> usize {
    DEFAULT_LIMIT
}

/// The order of the files in a result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortBy {
    /// By path, which also groups files by directory.
    #[default]
    Path,
    /// Most matching lines first.
    Matches,
    /// Most recently modified first. Needs the file system, so only a search over the project
    /// folder can do it; elsewhere this is the path order.
    Modified,
}

/// What to search for and where.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindRequest {
    pub query: String,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub whole_words: bool,
    #[serde(default)]
    pub regex: bool,
    /// Comma-separated globs; empty means every file.
    #[serde(default)]
    pub file_mask: String,
    /// Project-relative directory to search in (the "Directory" scope); empty = project.
    #[serde(default)]
    pub directory: String,
    /// Search only these project-relative files (open or changed files); `None` = no restriction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<String>>,
    /// A scope (predefined or a scope pattern, see `forge-scopes`); `None` = the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<forge_scopes::ScopeFilter>,
    /// Lines of context kept before and after each matching line (at most [`MAX_CONTEXT`]).
    #[serde(default)]
    pub context: usize,
    /// How the files of the result are ordered.
    #[serde(default)]
    pub sort: SortBy,
    /// Maximum number of matches reported.
    #[serde(default = "default_limit")]
    pub limit: usize,
}

impl Default for FindRequest {
    fn default() -> Self {
        FindRequest {
            query: String::new(),
            case_sensitive: false,
            whole_words: false,
            regex: false,
            file_mask: String::new(),
            directory: String::new(),
            paths: None,
            scope: None,
            context: 0,
            sort: SortBy::Path,
            limit: DEFAULT_LIMIT,
        }
    }
}

/// One line with at least one match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineMatch {
    /// 0-based line number.
    pub line: usize,
    /// UTF-16 column of the first match in the original line.
    pub column: usize,
    /// UTF-16 length of the first match.
    pub length: usize,
    /// The line as shown in the result list (leading whitespace removed, long lines cut).
    pub text: String,
    /// UTF-16 `[start, end)` ranges of the matches inside `text`.
    pub ranges: Vec<(usize, usize)>,
    /// The lines just before this one, when the request asked for context.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<String>,
    /// The lines just after this one, when the request asked for context.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
}

/// One line a replacement would change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplacePreview {
    /// 0-based line number.
    pub line: usize,
    pub before: String,
    pub after: String,
}

/// The matches of one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMatches {
    /// Project-relative path with `/` separators.
    pub path: String,
    pub matches: Vec<LineMatch>,
}

/// The answer to a [`FindRequest`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindResult {
    pub files: Vec<FileMatches>,
    /// Number of matches (occurrences, not lines).
    pub match_count: usize,
    /// More matches exist than were reported.
    pub truncated: bool,
    /// Why the query cannot run (an invalid regular expression); no files then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The first page of a [`FindResult`] with the totals, for lists that load the rest on demand.
/// Rows are matching lines, as the result list shows them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindPaged {
    /// Identifies the stored result later pages are cut from.
    pub id: u64,
    pub match_count: usize,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub file_count: usize,
    pub row_count: usize,
    pub files: Vec<FileMatches>,
}

impl FindResult {
    /// The rows (matching lines) `[offset, offset + count)` in result order. A file split across
    /// the page edge appears in both pages, each with its share of the lines.
    pub fn page(&self, offset: usize, count: usize) -> Vec<FileMatches> {
        let mut out = Vec::new();
        let (mut skip, mut left) = (offset, count);
        for file in &self.files {
            if left == 0 {
                break;
            }
            if skip >= file.matches.len() {
                skip -= file.matches.len();
                continue;
            }
            let take = (file.matches.len() - skip).min(left);
            out.push(FileMatches {
                path: file.path.clone(),
                matches: file.matches[skip..skip + take].to_vec(),
            });
            left -= take;
            skip = 0;
        }
        out
    }

    pub fn row_count(&self) -> usize {
        self.files.iter().map(|f| f.matches.len()).sum()
    }

    /// The totals plus the first `page_size` rows.
    pub fn paged(&self, id: u64, page_size: usize) -> FindPaged {
        FindPaged {
            id,
            match_count: self.match_count,
            truncated: self.truncated,
            error: self.error.clone(),
            file_count: self.files.len(),
            row_count: self.row_count(),
            files: self.page(0, page_size),
        }
    }
}

/// A compiled file mask.
#[derive(Debug, Clone, Default)]
pub struct FileMask {
    include: Vec<String>,
    exclude: Vec<String>,
}

impl FileMask {
    pub fn parse(mask: &str) -> Self {
        let mut out = FileMask::default();
        for part in mask
            .split([',', ';'])
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            match part.strip_prefix('!') {
                Some(p) => out.exclude.push(p.trim().to_string()),
                None => out.include.push(part.to_string()),
            }
        }
        out
    }

    /// Whether a project-relative path passes the mask.
    pub fn matches(&self, path: &str) -> bool {
        let name = path.rsplit('/').next().unwrap_or(path);
        let hit = |glob: &String| {
            if glob.contains('/') {
                glob_match(glob.trim_start_matches('/'), path)
            } else {
                glob_match(glob, name)
            }
        };
        (self.include.is_empty() || self.include.iter().any(hit)) && !self.exclude.iter().any(hit)
    }
}

/// `*` (any run without `/`, `**` any run), `?` (one character), case-insensitive.
pub fn glob_match(glob: &str, text: &str) -> bool {
    fn go(g: &[char], t: &[char]) -> bool {
        match g.first() {
            None => t.is_empty(),
            Some('*') => {
                let double = g.get(1) == Some(&'*');
                let rest = if double { &g[2..] } else { &g[1..] };
                for i in 0..=t.len() {
                    if go(rest, &t[i..]) {
                        return true;
                    }
                    if i < t.len() && t[i] == '/' && !double {
                        return false;
                    }
                }
                false
            }
            Some('?') => !t.is_empty() && t[0] != '/' && go(&g[1..], &t[1..]),
            Some(c) => {
                !t.is_empty() && c.to_lowercase().eq(t[0].to_lowercase()) && go(&g[1..], &t[1..])
            }
        }
    }
    let g: Vec<char> = glob.chars().collect();
    let t: Vec<char> = text.chars().collect();
    go(&g, &t)
}

/// The regular expression text a request compiles to (whole-word anchors applied).
pub fn pattern(req: &FindRequest) -> String {
    let mut pattern = if req.regex {
        req.query.clone()
    } else {
        regex::escape(&req.query)
    };
    if req.whole_words {
        let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
        // Only anchor the ends that are word characters (PHPStorm: `$user` still matches).
        let start = req.regex || word(req.query.chars().next());
        let end = req.regex || word(req.query.chars().last());
        pattern = format!(
            "{}(?:{pattern}){}",
            if start { r"\b" } else { "" },
            if end { r"\b" } else { "" }
        );
    }
    pattern
}

/// A compiled request.
#[derive(Debug, Clone)]
pub struct Finder {
    re: Regex,
    mask: FileMask,
    directory: String,
    only: Option<std::collections::HashSet<String>>,
    scope: Option<forge_scopes::Scope>,
    context: usize,
    sort: SortBy,
    regex: bool,
    limit: usize,
}

impl Finder {
    /// Compile `req`; an empty query or an invalid regular expression is an error.
    pub fn new(req: &FindRequest) -> Result<Self, String> {
        if req.query.is_empty() {
            return Err("Enter text to search for".into());
        }
        let pattern = pattern(req);
        let re = RegexBuilder::new(&pattern)
            .case_insensitive(!req.case_sensitive)
            .build()
            .map_err(|e| format!("Bad regular expression: {e}"))?;
        let directory = req.directory.trim_matches('/').to_string();
        let scope = match &req.scope {
            Some(filter) => Some(
                forge_scopes::Scope::new(filter).map_err(|e| format!("Bad scope pattern: {e}"))?,
            ),
            None => None,
        };
        Ok(Finder {
            scope,
            re,
            mask: FileMask::parse(&req.file_mask),
            directory,
            only: req.paths.as_ref().map(|p| {
                p.iter()
                    .map(|p| p.trim_start_matches('/').to_string())
                    .collect()
            }),
            context: req.context.min(MAX_CONTEXT),
            sort: req.sort,
            regex: req.regex,
            limit: req.limit.max(1),
        })
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    /// The scope directory (project-relative, no slashes around it; empty = project).
    pub fn directory(&self) -> &str {
        &self.directory
    }

    /// Whether `path` is in scope (directory and file mask).
    pub fn wants(&self, path: &str) -> bool {
        let in_dir = self.directory.is_empty()
            || path == self.directory
            || path
                .strip_prefix(self.directory.as_str())
                .is_some_and(|rest| rest.starts_with('/'));
        in_dir
            && self.only.as_ref().is_none_or(|only| only.contains(path))
            && self.scope.as_ref().is_none_or(|s| s.contains(path))
            && self.mask.matches(path)
    }

    /// Matching lines of one file's text, at most `budget` matches.
    pub fn search(&self, text: &str, budget: usize) -> (Vec<LineMatch>, usize, bool) {
        let mut out = Vec::new();
        let mut count = 0;
        for (line_no, line) in text.split('\n').enumerate() {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let found: Vec<(usize, usize)> = self
                .re
                .find_iter(line)
                .filter(|m| m.end() > m.start())
                .map(|m| (m.start(), m.end()))
                .collect();
            if found.is_empty() {
                continue;
            }
            if count + found.len() > budget {
                add_context(text, &mut out, self.context);
                return (out, count, true);
            }
            count += found.len();
            out.push(line_match(line_no, line, &found));
        }
        add_context(text, &mut out, self.context);
        (out, count, false)
    }

    /// `text` with every match replaced by `with`, and how many were replaced. Matches are found
    /// line by line like [`Finder::search`]; line breaks are kept as they are. In regular
    /// expression mode `with` may use `$1`, `${name}` and `$$`; otherwise it is inserted as typed.
    pub fn replace_text(&self, text: &str, with: &str) -> (String, usize) {
        let mut out = String::with_capacity(text.len());
        let mut count = 0;
        for piece in text.split_inclusive('\n') {
            let body = piece.strip_suffix('\n').unwrap_or(piece);
            let line = body.strip_suffix('\r').unwrap_or(body);
            let (replaced, n) = self.replace_line(line, with);
            out.push_str(&replaced);
            out.push_str(&piece[line.len()..]);
            count += n;
        }
        (out, count)
    }

    fn replace_line(&self, line: &str, with: &str) -> (String, usize) {
        let mut out = String::with_capacity(line.len());
        let (mut at, mut count) = (0, 0);
        for caps in self.re.captures_iter(line) {
            let Some(m) = caps.get(0).filter(|m| m.end() > m.start()) else {
                continue;
            };
            out.push_str(&line[at..m.start()]);
            if self.regex {
                caps.expand(with, &mut out);
            } else {
                out.push_str(with);
            }
            at = m.end();
            count += 1;
        }
        out.push_str(&line[at..]);
        (out, count)
    }

    /// The matching lines of `text` as they would read after replacing: `(line, before, after)`,
    /// at most `budget` lines. Both texts are trimmed and cut like result rows.
    pub fn replace_preview(&self, text: &str, with: &str, budget: usize) -> Vec<ReplacePreview> {
        let mut out = Vec::new();
        for (line_no, line) in text.split('\n').enumerate() {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let (after, n) = self.replace_line(line, with);
            if n == 0 {
                continue;
            }
            if out.len() >= budget {
                break;
            }
            let shown = |s: &str| s.trim().chars().take(PREVIEW_CHARS).collect::<String>();
            out.push(ReplacePreview {
                line: line_no,
                before: shown(line),
                after: shown(&after),
            });
        }
        out
    }

    /// Lines of context requested around each matching line.
    pub fn context(&self) -> usize {
        self.context
    }

    pub fn sort(&self) -> SortBy {
        self.sort
    }
}

/// The rows of a file whose match ranges (byte offsets into `text`, none spanning a line break)
/// were found elsewhere, e.g. by a faster regex engine. Same result as [`Finder::search`] with
/// the same budget, provided the ranges are the non-empty matches of the request's pattern.
pub fn lines_from_ranges(
    text: &str,
    ranges: &[(usize, usize)],
    budget: usize,
) -> (Vec<LineMatch>, usize, bool) {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut count = 0;
    let (mut line_no, mut line_start, mut counted) = (0, 0, 0);
    let mut i = 0;
    while i < ranges.len() {
        let at = ranges[i].0;
        let skipped = &bytes[counted..at];
        line_no += skipped.iter().filter(|&&b| b == b'\n').count();
        if let Some(p) = skipped.iter().rposition(|&b| b == b'\n') {
            line_start = counted + p + 1;
        }
        counted = at;
        let next_line = bytes[at..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(bytes.len(), |p| at + p);
        let mut line_end = next_line;
        if line_end > line_start && bytes[line_end - 1] == b'\r' {
            line_end -= 1;
        }
        let mut found = Vec::new();
        while i < ranges.len() && ranges[i].0 <= next_line {
            found.push((ranges[i].0 - line_start, ranges[i].1 - line_start));
            i += 1;
        }
        if count + found.len() > budget {
            return (out, count, true);
        }
        count += found.len();
        out.push(line_match(line_no, &text[line_start..line_end], &found));
    }
    (out, count, false)
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// The result row of a line: trimmed, cut around the first match, ranges in UTF-16.
fn line_match(line_no: usize, line: &str, found: &[(usize, usize)]) -> LineMatch {
    let (first_start, first_end) = found[0];
    // Indentation and trailing whitespace are left out, unless a match is in them (a search
    // for a tab or for trailing spaces): then the text keeps them, so the match is highlighted.
    let last_end = found.iter().map(|&(_, e)| e).max().unwrap_or(first_end);
    let mut start = (line.len() - line.trim_start().len()).min(first_start);
    let mut end = line
        .trim_end()
        .len()
        .max(last_end.min(line.len()))
        .max(start);
    // Long lines: keep some context before the first match.
    if line[start..end].chars().count() > PREVIEW_CHARS {
        let before: Vec<(usize, char)> =
            line[start..first_start.max(start)].char_indices().collect();
        if before.len() > 40 {
            start += before[before.len() - 40].0;
        }
        let cut = line[start..]
            .char_indices()
            .nth(PREVIEW_CHARS)
            .map_or(end, |(i, _)| (start + i).min(end));
        end = cut.max(first_end.min(line.len()));
    }
    let text = &line[start..end];
    let ranges = found
        .iter()
        .filter(|(s, e)| *s >= start && *e <= end)
        .map(|(s, e)| (utf16_len(&line[start..*s]), utf16_len(&line[start..*e])))
        .collect();
    LineMatch {
        line: line_no,
        column: utf16_len(&line[..first_start]),
        length: utf16_len(&line[first_start..first_end]),
        text: text.to_string(),
        ranges,
        before: Vec::new(),
        after: Vec::new(),
    }
}

/// Most context lines a request may ask for.
pub const MAX_CONTEXT: usize = 5;

/// Fills `before` and `after` of `rows` with up to `context` neighbouring lines of `text`.
pub fn add_context(text: &str, rows: &mut [LineMatch], context: usize) {
    let context = context.min(MAX_CONTEXT);
    if context == 0 || rows.is_empty() {
        return;
    }
    let lines: Vec<&str> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let shown = |line: &&str| {
        let trimmed = line.trim();
        trimmed.chars().take(PREVIEW_CHARS).collect::<String>()
    };
    for row in rows {
        let from = row.line.saturating_sub(context);
        row.before = lines[from..row.line.min(lines.len())]
            .iter()
            .map(shown)
            .collect();
        let to = (row.line + 1 + context).min(lines.len());
        row.after = lines[(row.line + 1).min(to)..to]
            .iter()
            .map(shown)
            .collect();
    }
}

/// Whether file content looks binary (a NUL byte near the start).
pub fn is_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(8192)].contains(&0)
}

/// Collects file results up to the request's limit.
#[derive(Debug)]
pub struct Collector<'a> {
    finder: &'a Finder,
    result: FindResult,
}

impl<'a> Collector<'a> {
    pub fn new(finder: &'a Finder) -> Self {
        Collector {
            finder,
            result: FindResult::default(),
        }
    }

    /// Whether the limit is reached (stop walking).
    pub fn full(&self) -> bool {
        self.result.truncated
    }

    /// Search one file (already known to be in scope).
    pub fn add(&mut self, path: &str, text: &str) {
        if self.full() {
            return;
        }
        let budget = self.finder.limit - self.result.match_count;
        let (matches, count, truncated) = self.finder.search(text, budget);
        self.result.match_count += count;
        self.result.truncated |= truncated;
        if !matches.is_empty() {
            self.result.files.push(FileMatches {
                path: path.to_string(),
                matches,
            });
        }
    }

    pub fn finish(mut self) -> FindResult {
        self.result.files.sort_by(|a, b| a.path.cmp(&b.path));
        if self.finder.sort == SortBy::Matches {
            self.result
                .files
                .sort_by_key(|f| std::cmp::Reverse(f.matches.len()));
        }
        self.result
    }
}

/// Search in-memory files (browser mode, tests): every `(path, text)` in scope.
pub fn find_in<'t>(
    req: &FindRequest,
    files: impl IntoIterator<Item = (&'t str, &'t str)>,
) -> FindResult {
    let finder = match Finder::new(req) {
        Ok(f) => f,
        Err(e) => {
            return FindResult {
                error: Some(e),
                ..FindResult::default()
            }
        }
    };
    let mut collector = Collector::new(&finder);
    for (path, text) in files {
        if collector.full() {
            break;
        }
        if finder.wants(path) {
            collector.add(path, text);
        }
    }
    collector.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(query: &str) -> FindRequest {
        FindRequest {
            query: query.into(),
            ..FindRequest::default()
        }
    }

    const USER: &str = "<?php\nclass User\n{\n    public function fullName(): string\n    {\n        return $this->first_name . ' ' . $this->last_name; // user name\n    }\n}\n";

    #[test]
    fn finds_literals_case_insensitively_by_default() {
        let r = find_in(&req("user"), [("app/Models/User.php", USER)]);
        assert_eq!(r.match_count, 2);
        let m = &r.files[0].matches;
        assert_eq!((m[0].line, m[0].column, m[0].length), (1, 6, 4));
        assert_eq!(m[0].text, "class User");
        assert_eq!(m[0].ranges, vec![(6, 10)]);
        // Leading whitespace is trimmed; ranges are relative to the trimmed text.
        assert!(m[1].text.starts_with("return $this"));
        let (s, e) = m[1].ranges[0];
        assert_eq!(&m[1].text[s..e], "user");
    }

    #[test]
    fn case_words_and_regex_options() {
        let files = [("a.php", USER)];
        let mut q = req("User");
        q.case_sensitive = true;
        assert_eq!(find_in(&q, files).match_count, 1);

        let mut q = req("name");
        assert_eq!(find_in(&q, files).match_count, 4); // fullName, first_name, last_name, name
        q.whole_words = true;
        assert_eq!(find_in(&q, files).match_count, 1);

        let mut q = req(r"\$this->(\w+)_name");
        q.regex = true;
        let r = find_in(&q, files);
        assert_eq!(r.match_count, 2);
        assert_eq!(r.files[0].matches.len(), 1, "both on one line");
        assert_eq!(r.files[0].matches[0].ranges.len(), 2);

        // `$this` with whole words still matches (the `$` end is not a word character).
        let mut q = req("$this");
        q.whole_words = true;
        assert_eq!(find_in(&q, files).match_count, 2);

        let mut q = req("(unclosed");
        q.regex = true;
        let r = find_in(&q, files);
        assert!(r
            .error
            .as_deref()
            .unwrap()
            .starts_with("Bad regular expression"));
        assert!(find_in(&req(""), files).error.is_some());
    }

    #[test]
    fn masks_and_directories() {
        let mask = FileMask::parse("*.php, !*Test.php");
        assert!(mask.matches("app/Models/User.php"));
        assert!(!mask.matches("tests/UserTest.php"));
        assert!(!mask.matches("resources/app.js"));
        assert!(FileMask::parse("").matches("anything"));
        assert!(FileMask::parse("app/**/*.php").matches("app/Models/User.php"));
        assert!(!FileMask::parse("app/*.php").matches("app/Models/User.php"));
        assert!(FileMask::parse("*.PHP").matches("a.php"));
        assert!(glob_match("Us?r.*", "User.php"));

        let files = [
            ("app/Models/User.php", "User"),
            ("tests/UserTest.php", "User"),
            ("application/User.php", "User"),
        ];
        let mut q = req("User");
        q.directory = "app/".into();
        let r = find_in(&q, files);
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.files[0].path, "app/Models/User.php");
        q.directory.clear();
        q.file_mask = "!*Test.php".into();
        assert_eq!(find_in(&q, files).files.len(), 2);

        let mut q = req("User");
        q.paths = Some(vec!["tests/UserTest.php".into()]);
        let r = find_in(&q, files);
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.files[0].path, "tests/UserTest.php");
        q.paths = Some(vec![]);
        assert!(find_in(&q, files).files.is_empty());

        // Scopes from the Scopes page (`forge-scopes`).
        let mut q = req("User");
        q.scope = Some(forge_scopes::ScopeFilter {
            scope: forge_scopes::ScopeSpec::Pattern("file:app*//*&&!test:*..*".into()),
            ..Default::default()
        });
        let r = find_in(&q, files);
        let paths: Vec<&str> = r.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["app/Models/User.php", "application/User.php"]);
        q.scope = Some(forge_scopes::ScopeFilter {
            scope: forge_scopes::ScopeSpec::Predefined(forge_scopes::Predefined::ProjectTestFiles),
            ..Default::default()
        });
        assert_eq!(find_in(&q, files).files[0].path, "tests/UserTest.php");
        q.scope = Some(forge_scopes::ScopeFilter {
            scope: forge_scopes::ScopeSpec::Pattern("file:a&&".into()),
            ..Default::default()
        });
        assert!(find_in(&q, files)
            .error
            .unwrap()
            .starts_with("Bad scope pattern"));
    }

    #[test]
    fn context_lines_surround_a_match_and_stop_at_the_edges() {
        let mut q = req("hit");
        q.context = 2;
        let text = "a\n  b\nhit\r\nd\ne\nf\n";
        let r = find_in(&q, [("x.txt", text)]);
        let row = &r.files[0].matches[0];
        assert_eq!(row.before, ["a", "b"]);
        assert_eq!(row.after, ["d", "e"]);
        let r = find_in(&req("a"), [("x.txt", text)]);
        assert!(r.files[0].matches[0].before.is_empty());
        let mut q = req("a");
        q.context = 3;
        let first = &find_in(&q, [("x.txt", text)]).files[0].matches[0];
        assert!(first.before.is_empty());
        assert_eq!(first.after, ["b", "hit", "d"]);
    }

    #[test]
    fn files_can_be_ordered_by_their_matching_lines() {
        let files = [
            ("a.txt", "x\n"),
            ("b.txt", "x\nx\nx\n"),
            ("c.txt", "x\nx\n"),
        ];
        let mut q = req("x");
        let paths = |r: &FindResult| r.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
        assert_eq!(paths(&find_in(&q, files)), ["a.txt", "b.txt", "c.txt"]);
        q.sort = SortBy::Matches;
        assert_eq!(paths(&find_in(&q, files)), ["b.txt", "c.txt", "a.txt"]);
    }

    #[test]
    fn replacing_keeps_line_breaks_and_expands_groups_only_in_regex_mode() {
        let finder = Finder::new(&req("user")).unwrap();
        let (out, n) = finder.replace_text("a user\r\nUser $1\nnone", "$1x");
        assert_eq!(out, "a $1x\r\n$1x $1\nnone");
        assert_eq!(n, 2);

        let mut q = req(r"(\w+)@(\w+)");
        q.regex = true;
        let finder = Finder::new(&q).unwrap();
        let (out, n) = finder.replace_text("me@home and you@work\n", "$2:$1");
        assert_eq!(out, "home:me and work:you\n");
        assert_eq!(n, 2);

        let mut q = req("a*");
        q.regex = true;
        let finder = Finder::new(&q).unwrap();
        assert_eq!(finder.replace_text("baab", "X"), ("bXb".to_string(), 1));

        let mut q = req("id");
        q.whole_words = true;
        let finder = Finder::new(&q).unwrap();
        assert_eq!(
            finder.replace_text("id userid id\n", "key").0,
            "key userid key\n"
        );
    }

    #[test]
    fn a_replace_preview_lists_the_changed_lines_only() {
        let finder = Finder::new(&req("old")).unwrap();
        let rows = finder.replace_preview("keep\n  old name\nkeep\nold\n", "new", 10);
        assert_eq!(
            rows,
            vec![
                ReplacePreview {
                    line: 1,
                    before: "old name".into(),
                    after: "new name".into()
                },
                ReplacePreview {
                    line: 3,
                    before: "old".into(),
                    after: "new".into()
                },
            ]
        );
        assert_eq!(finder.replace_preview("old\nold\n", "x", 1).len(), 1);
    }

    #[test]
    fn limits_and_previews() {
        let text = "a a a\na\na\n";
        let mut q = req("a");
        q.limit = 4;
        let r = find_in(&q, [("x.txt", text)]);
        assert!(r.truncated);
        assert_eq!(r.match_count, 4);
        assert_eq!(r.files[0].matches.len(), 2);

        // Long lines are cut around the first match; UTF-16 columns.
        let long = format!("{}€ needle {}", "x".repeat(300), "y".repeat(300));
        let r = find_in(&req("needle"), [("l.txt", long.as_str())]);
        let m = &r.files[0].matches[0];
        assert_eq!(m.column, 302);
        assert!(m.text.chars().count() <= PREVIEW_CHARS + 1);
        let (s, e) = m.ranges[0];
        let utf16: Vec<u16> = m.text.encode_utf16().collect();
        assert_eq!(String::from_utf16(&utf16[s..e]).unwrap(), "needle");

        // CRLF and zero-width matches.
        let mut q = req("^");
        q.regex = true;
        assert_eq!(find_in(&q, [("e.txt", "a\r\nb")]).match_count, 0);
        assert!(is_binary(b"ab\0c"));
        assert!(!is_binary(b"abc"));
    }

    #[test]
    fn matches_in_indentation_and_trailing_whitespace_are_highlighted() {
        let mut q = req("\t");
        q.regex = true;
        let finder = Finder::new(&q).unwrap();
        let (rows, _, _) = finder.search("\tfoo();", 10);
        assert_eq!(
            (rows[0].text.as_str(), rows[0].ranges.clone()),
            ("\tfoo();", vec![(0, 1)])
        );
        let mut q = req("\\s+$");
        q.regex = true;
        let finder = Finder::new(&q).unwrap();
        let (rows, _, _) = finder.search("  let x = 1;   ", 10);
        assert_eq!(rows[0].text, "let x = 1;   ");
        assert_eq!(rows[0].ranges, vec![(10, 13)]);
        // Other lines keep their indentation trimmed.
        let (rows, _, _) = Finder::new(&req("foo")).unwrap().search("    foo  ", 10);
        assert_eq!(
            (rows[0].text.as_str(), rows[0].ranges.clone()),
            ("foo", vec![(0, 3)])
        );
    }

    #[test]
    fn a_match_inside_the_indent_of_a_long_line_does_not_panic() {
        let line = format!("    {}", "word ".repeat(80));
        let mut request = req(".*");
        request.regex = true;
        let finder = Finder::new(&request).unwrap();
        let (rows, count, _) = finder.search(&line, 10);
        assert_eq!((rows.len(), count), (1, 1));
        let (rows, count, _) = lines_from_ranges(&line, &[(0, line.len())], 10);
        assert_eq!((rows.len(), count), (1, 1));
    }

    #[test]
    fn pages_cut_rows_across_files() {
        let files = [
            ("a.php", "x\nx\nx\n"),
            ("b.php", "x\n"),
            ("c.php", "x\nx\n"),
        ];
        let result = find_in(&req("x"), files);
        assert_eq!(result.row_count(), 6);
        let rows = |page: Vec<FileMatches>| -> Vec<(String, usize)> {
            page.into_iter()
                .map(|f| (f.path, f.matches.len()))
                .collect()
        };
        assert_eq!(rows(result.page(0, 2)), [("a.php".to_string(), 2)]);
        assert_eq!(
            rows(result.page(2, 3)),
            [
                ("a.php".to_string(), 1),
                ("b.php".to_string(), 1),
                ("c.php".to_string(), 1)
            ]
        );
        assert_eq!(rows(result.page(5, 10)), [("c.php".to_string(), 1)]);
        assert!(result.page(6, 10).is_empty());
        let first = result.paged(7, 4);
        assert_eq!((first.id, first.row_count, first.file_count), (7, 6, 3));
        assert_eq!(
            first.files.iter().map(|f| f.matches.len()).sum::<usize>(),
            4
        );
    }

    #[test]
    fn ranges_give_the_same_rows_as_a_line_search() {
        let text = "alpha beta\r\n\nbeta beta gamma\nno\n  beta\tbeta beta\n";
        let finder = Finder::new(&req("beta")).unwrap();
        let ranges: Vec<(usize, usize)> = text
            .match_indices("beta")
            .map(|(s, m)| (s, s + m.len()))
            .collect();
        for budget in [0, 1, 2, 3, 4, 5, 6, 100] {
            assert_eq!(
                lines_from_ranges(text, &ranges, budget),
                finder.search(text, budget),
                "budget {budget}"
            );
        }
        assert_eq!(lines_from_ranges(text, &[], 10), (vec![], 0, false));
    }

    #[test]
    fn serializes_camel_case() {
        let r: FindRequest =
            serde_json::from_str(r#"{"query":"x","caseSensitive":true,"fileMask":"*.php"}"#)
                .unwrap();
        assert!(r.case_sensitive);
        assert_eq!(r.limit, DEFAULT_LIMIT);
        let out = serde_json::to_string(&find_in(&r, [("a.php", "x")])).unwrap();
        assert!(out.contains("\"matchCount\":1"), "{out}");
    }
}
