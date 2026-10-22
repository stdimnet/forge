//! Structural Search and Replace (PHPStorm's Edit | Find | Search Structurally… and
//! Replace Structurally…).
//!
//! A template is code with `$variables$` (`$Instance$->$Method$($Args$)`). It is parsed with
//! the file type's tree-sitter grammar ([`template`]); every file of the scope is parsed too
//! and the template's tree is matched against each node ([`matcher`]): node kinds and
//! tokens must be equal, a variable stands for any node, or for a run of sibling nodes when
//! its count allows (`$Args$` with count 0..∞ matches every argument list). Variables may be
//! constrained by a regular expression on their text and, where the type is known (literals,
//! `new`, typed parameters, `$this`), on their type. A variable used twice must match the
//! same text both times.
//!
//! Replacing fills the replacement template with the matched variables' text. The
//! [`library`] holds PHPStorm's built-in templates.

pub mod library;
#[cfg(feature = "engine")]
mod matcher;
pub mod template;
#[cfg(feature = "engine")]
mod types;

use std::collections::BTreeMap;
#[cfg(feature = "engine")]
use std::collections::HashMap;

#[cfg(feature = "engine")]
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

pub use template::variables;

/// The languages templates are written in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileType {
    #[default]
    Php,
    JavaScript,
    TypeScript,
}

impl FileType {
    pub const ALL: [FileType; 3] = [FileType::Php, FileType::JavaScript, FileType::TypeScript];

    pub fn label(self) -> &'static str {
        match self {
            FileType::Php => "PHP",
            FileType::JavaScript => "JavaScript",
            FileType::TypeScript => "TypeScript",
        }
    }

    /// File name extensions searched for this file type.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            FileType::Php => &["php", "phtml", "inc"],
            FileType::JavaScript => &["js", "jsx", "mjs", "cjs"],
            FileType::TypeScript => &["ts", "tsx", "mts", "cts"],
        }
    }

    #[cfg(feature = "engine")]
    fn grammars(self) -> &'static [Grammar] {
        match self {
            FileType::Php => &[Grammar::Php],
            FileType::JavaScript => &[Grammar::JavaScript],
            FileType::TypeScript => &[Grammar::TypeScript, Grammar::Tsx],
        }
    }

    /// The grammar a file of this type is parsed with; `None` for other files.
    #[cfg(feature = "engine")]
    pub(crate) fn grammar_for(self, path: &str) -> Option<Grammar> {
        let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
        if name.ends_with(".blade.php") {
            return None;
        }
        let ext = name.rsplit_once('.')?.1;
        if !self.extensions().contains(&ext) {
            return None;
        }
        Some(match self {
            FileType::Php => Grammar::Php,
            FileType::JavaScript => Grammar::JavaScript,
            FileType::TypeScript if ext == "tsx" => Grammar::Tsx,
            FileType::TypeScript => Grammar::TypeScript,
        })
    }
}

#[cfg(feature = "engine")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Grammar {
    Php,
    JavaScript,
    TypeScript,
    Tsx,
}

#[cfg(feature = "engine")]
impl Grammar {
    fn language_label(self) -> &'static str {
        match self {
            Grammar::Php => "PHP",
            Grammar::JavaScript => "JavaScript",
            Grammar::TypeScript | Grammar::Tsx => "TypeScript",
        }
    }
}

fn one() -> u32 {
    1
}

fn one_max() -> Option<u32> {
    Some(1)
}

/// The modifiers of one template variable (the Filters panel of the dialog).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Constraint {
    pub name: String,
    /// Count: how many nodes the variable stands for, `min_count..=max_count`.
    #[serde(default = "one")]
    pub min_count: u32,
    /// `None`: unlimited.
    #[serde(default = "one_max")]
    pub max_count: Option<u32>,
    /// Text: a regular expression the whole text must match; empty = any.
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub invert_text: bool,
    /// Type: a regular expression for the expression's type (PHP class names, `string`,
    /// `int`, …); empty = any.
    #[serde(default, rename = "type")]
    pub type_: String,
    #[serde(default)]
    pub invert_type: bool,
}

impl Constraint {
    pub fn named(name: &str) -> Self {
        Constraint {
            name: name.into(),
            min_count: 1,
            max_count: Some(1),
            text: String::new(),
            invert_text: false,
            type_: String::new(),
            invert_type: false,
        }
    }

    /// Count `min..` (unlimited).
    pub fn count(mut self, min: u32, max: Option<u32>) -> Self {
        self.min_count = min;
        self.max_count = max;
        self
    }
}

fn default_limit() -> usize {
    1000
}

fn yes() -> bool {
    true
}

/// A structural search (and replace).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsrRequest {
    pub template: String,
    #[serde(default)]
    pub file_type: FileType,
    /// Modifiers of the variables; variables without an entry match one node of any text.
    #[serde(default)]
    pub constraints: Vec<Constraint>,
    #[serde(default = "yes")]
    pub match_case: bool,
    /// The replacement template (Replace Structurally).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
    /// Where to search; `None` = Project Files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<forge_scopes::ScopeFilter>,
    /// Unsaved editor contents by path; they replace the files on disk.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub buffers: BTreeMap<String, String>,
    /// Maximum number of matches reported.
    #[serde(default = "default_limit")]
    pub limit: usize,
}

impl Default for SsrRequest {
    fn default() -> Self {
        SsrRequest {
            template: String::new(),
            file_type: FileType::Php,
            constraints: Vec::new(),
            match_case: true,
            replacement: None,
            scope: None,
            buffers: BTreeMap::new(),
            limit: default_limit(),
        }
    }
}

/// What a variable matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub name: String,
    pub text: String,
}

/// One match, positioned like a Find in Files row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsrMatch {
    /// 0-based line and UTF-16 column of the start and the end.
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    /// Byte range in the file.
    pub start: usize,
    pub end: usize,
    /// The first line of the match, leading whitespace removed and long lines cut.
    pub text: String,
    /// UTF-16 `[start, end)` of the match inside `text`.
    pub ranges: Vec<(usize, usize)>,
    /// The matched code.
    pub matched: String,
    pub bindings: Vec<Binding>,
    /// The code that replaces the match (with a replacement template).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsrFile {
    pub path: String,
    pub matches: Vec<SsrMatch>,
}

/// The answer to a [`SsrRequest`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsrResult {
    pub files: Vec<SsrFile>,
    pub match_count: usize,
    /// Files of the file type in the scope.
    pub searched_files: usize,
    pub truncated: bool,
    /// Why the search cannot run (an invalid template or modifier); no files then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// What the dialog shows while a template is typed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateCheck {
    pub variables: Vec<String>,
    /// Variables of the replacement template the search template does not define.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_in_replacement: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(feature = "engine")]
pub(crate) struct VarSpec {
    pub name: String,
    pub min: u32,
    pub max: Option<u32>,
    pub text: Option<Regex>,
    pub invert_text: bool,
    pub type_re: Option<Regex>,
    pub invert_type: bool,
}

#[cfg(feature = "engine")]
/// A compiled request, ready to match files.
pub struct Compiled {
    file_type: FileType,
    patterns: HashMap<Grammar, template::Pattern>,
    pub(crate) vars: Vec<VarSpec>,
    pub(crate) match_case: bool,
    replacement: Option<String>,
}

#[cfg(feature = "engine")]
fn anchored(re: &str, what: &str, name: &str, case: bool) -> Result<Option<Regex>, String> {
    if re.trim().is_empty() {
        return Ok(None);
    }
    RegexBuilder::new(&format!("^(?:{re})$"))
        .case_insensitive(!case)
        .build()
        .map(Some)
        .map_err(|e| format!("Invalid {what} regular expression for ${name}$: {e}"))
}

#[cfg(feature = "engine")]
impl Compiled {
    pub fn new(req: &SsrRequest) -> Result<Self, String> {
        let names = variables(&req.template);
        let mut vars = Vec::new();
        for name in &names {
            let c = req
                .constraints
                .iter()
                .find(|c| c.name == *name)
                .cloned()
                .unwrap_or_else(|| Constraint::named(name));
            if c.max_count.is_some_and(|m| m < c.min_count) {
                return Err(format!(
                    "The maximum count of ${name}$ is lower than its minimum"
                ));
            }
            vars.push(VarSpec {
                name: name.clone(),
                min: c.min_count,
                max: c.max_count,
                text: anchored(&c.text, "text", name, req.match_case)?,
                invert_text: c.invert_text,
                type_re: anchored(&c.type_, "type", name, false)?,
                invert_type: c.invert_type,
            });
        }
        let mut patterns = HashMap::new();
        for &g in req.file_type.grammars() {
            patterns.insert(g, template::compile(&req.template, g, &names)?);
        }
        Ok(Compiled {
            file_type: req.file_type,
            patterns,
            vars,
            match_case: req.match_case,
            replacement: req.replacement.clone(),
        })
    }

    pub fn file_type(&self) -> FileType {
        self.file_type
    }

    /// Whether a file is parsed for this search.
    pub fn wants(&self, path: &str) -> bool {
        self.file_type.grammar_for(path).is_some()
    }

    /// The matches in one file's text, in document order.
    pub fn search(&self, path: &str, text: &str) -> Vec<SsrMatch> {
        let Some(grammar) = self.file_type.grammar_for(path) else {
            return Vec::new();
        };
        let Some(pattern) = self.patterns.get(&grammar) else {
            return Vec::new();
        };
        let Some(tree) = template::parser(grammar).parse(text, None) else {
            return Vec::new();
        };
        let found = matcher::find(self, pattern, &tree, text);
        let lines = LineIndex::new(text);
        found
            .into_iter()
            .map(|m| {
                let replacement = self
                    .replacement
                    .as_deref()
                    .map(|r| fill(r, &self.vars, &m.bindings, text, m.start));
                lines.to_match(text, m, &self.vars, replacement)
            })
            .collect()
    }

    /// `text` with every match replaced (outermost first, overlapping matches skipped) and the
    /// number of replacements; `None` without a replacement template.
    pub fn replace(&self, path: &str, text: &str) -> Option<(String, usize)> {
        self.replacement.as_ref()?;
        let mut matches = self.search(path, text);
        matches.sort_by_key(|m| (m.start, std::cmp::Reverse(m.end)));
        let mut out = String::with_capacity(text.len());
        let (mut at, mut count) = (0, 0);
        for m in matches {
            if m.start < at {
                continue;
            }
            out.push_str(&text[at..m.start]);
            out.push_str(m.replacement.as_deref().unwrap_or(&m.matched));
            at = m.end;
            count += 1;
        }
        out.push_str(&text[at..]);
        Some((out, count))
    }
}

#[cfg(feature = "engine")]
/// Check a template while it is typed: its variables and why it does not compile.
pub fn check(req: &SsrRequest) -> TemplateCheck {
    let variables = variables(&req.template);
    let unknown_in_replacement = req
        .replacement
        .as_deref()
        .map(template::variables)
        .unwrap_or_default()
        .into_iter()
        .filter(|v| !variables.contains(v))
        .collect();
    let error = if req.template.trim().is_empty() {
        None
    } else {
        Compiled::new(req).err()
    };
    TemplateCheck {
        variables,
        unknown_in_replacement,
        error,
    }
}

#[cfg(feature = "engine")]
/// Search `files` (`(path, text)`; buffers of the request win) and collect the result.
pub fn search_files<'a>(
    req: &SsrRequest,
    files: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> SsrResult {
    let compiled = match Compiled::new(req) {
        Ok(c) => c,
        Err(e) => {
            return SsrResult {
                error: Some(e),
                ..SsrResult::default()
            }
        }
    };
    let scope = match req.scope.as_ref().map(forge_scopes::Scope::new).transpose() {
        Ok(s) => s,
        Err(e) => {
            return SsrResult {
                error: Some(format!("Bad scope pattern: {e}")),
                ..SsrResult::default()
            }
        }
    };
    let mut collector = Collector::new(req.limit);
    for (path, text) in files {
        if !compiled.wants(path) || scope.as_ref().is_some_and(|s| !s.contains(path)) {
            continue;
        }
        let text = req.buffers.get(path).map(String::as_str).unwrap_or(text);
        collector.add(path, compiled.search(path, text));
        if collector.full() {
            break;
        }
    }
    collector.finish()
}

/// Gathers per-file matches up to a limit.
pub struct Collector {
    result: SsrResult,
    limit: usize,
}

impl Collector {
    pub fn new(limit: usize) -> Self {
        Collector {
            result: SsrResult::default(),
            limit: limit.max(1),
        }
    }

    pub fn add(&mut self, path: &str, mut matches: Vec<SsrMatch>) {
        self.result.searched_files += 1;
        if matches.is_empty() {
            return;
        }
        let room = self.limit.saturating_sub(self.result.match_count);
        if matches.len() > room {
            matches.truncate(room);
            self.result.truncated = true;
        }
        if matches.is_empty() {
            return;
        }
        self.result.match_count += matches.len();
        self.result.files.push(SsrFile {
            path: path.to_string(),
            matches,
        });
    }

    pub fn full(&self) -> bool {
        self.result.truncated || self.result.match_count >= self.limit
    }

    pub fn finish(mut self) -> SsrResult {
        self.result.files.sort_by(|a, b| a.path.cmp(&b.path));
        self.result
    }
}

#[cfg(feature = "engine")]
/// The replacement template with the variables' text filled in; the template's own line
/// breaks get the indentation of the match's line.
fn fill(
    replacement: &str,
    vars: &[VarSpec],
    bindings: &[Option<(usize, usize)>],
    text: &str,
    start: usize,
) -> String {
    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let indent: String = text[line_start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let literal = |s: &str| s.replace('\n', &format!("\n{indent}"));
    let mut out = String::new();
    let mut at = 0;
    for caps in template::variable_re().captures_iter(replacement) {
        let whole = caps.get(0).expect("match");
        out.push_str(&literal(&replacement[at..whole.start()]));
        match vars.iter().position(|v| v.name == caps[1]) {
            Some(i) => {
                if let Some((s, e)) = bindings.get(i).copied().flatten() {
                    out.push_str(&text[s..e]);
                }
            }
            None => out.push_str(whole.as_str()),
        }
        at = whole.end();
    }
    out.push_str(&literal(&replacement[at..]));
    out
}

#[cfg(feature = "engine")]
/// Line starts of a text, for positions.
struct LineIndex {
    starts: Vec<usize>,
}

#[cfg(feature = "engine")]
impl LineIndex {
    fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex { starts }
    }

    fn line_of(&self, byte: usize) -> usize {
        self.starts.partition_point(|&s| s <= byte) - 1
    }

    fn utf16_col(&self, text: &str, line: usize, byte: usize) -> usize {
        text[self.starts[line]..byte].encode_utf16().count()
    }

    fn to_match(
        &self,
        text: &str,
        m: matcher::Found,
        vars: &[VarSpec],
        replacement: Option<String>,
    ) -> SsrMatch {
        let line = self.line_of(m.start);
        let end_line = self.line_of(m.end);
        let line_end = text[self.starts[line]..]
            .find('\n')
            .map_or(text.len(), |i| self.starts[line] + i);
        let raw = text[self.starts[line]..line_end].trim_end_matches('\r');
        let trimmed = raw.trim_start();
        let indent = raw.len() - trimmed.len();
        let start_in = m.start - self.starts[line] - indent;
        let end_in =
            (m.end.min(self.starts[line] + raw.len()) - self.starts[line]).saturating_sub(indent);
        let shown: String = trimmed.chars().take(PREVIEW_CHARS).collect();
        let to16 = |b: usize| shown[..b.min(shown.len())].encode_utf16().count();
        let ranges = vec![(to16(start_in), to16(end_in.max(start_in)))];
        let bindings = vars
            .iter()
            .zip(&m.bindings)
            .map(|(v, b)| Binding {
                name: v.name.clone(),
                text: b.map(|(s, e)| text[s..e].to_string()).unwrap_or_default(),
            })
            .collect();
        SsrMatch {
            line,
            column: self.utf16_col(text, line, m.start),
            end_line,
            end_column: self.utf16_col(text, end_line, m.end),
            start: m.start,
            end: m.end,
            text: shown,
            ranges,
            matched: text[m.start..m.end].to_string(),
            bindings,
            replacement,
        }
    }
}

#[cfg(feature = "engine")]
/// Longest line preview sent to the UI.
const PREVIEW_CHARS: usize = 220;
