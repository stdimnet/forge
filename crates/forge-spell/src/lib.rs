//! Spelling and grammar inspections (PHPStorm's "Typo" and "Grammar"): the editor sends the
//! comments, strings, identifiers and text of the visible or changed part of a file as
//! [`Segment`]s; [`inspect`] answers the problems with UTF-16 document offsets.
//! [`context_actions`] builds the Alt+Enter entries for problems at the caret ("Typo: Change
//! to...", "Typo: Rename to...", "Save 'word' to project-level dictionary", grammar fixes).
//!
//! - [`hunspell`]: a minimal Hunspell `.aff` / `.dic` reader (affix expansion).
//! - [`dictionary`]: the bundled en_US dictionary (SCOWL, `data/en_US.LICENSE`), the
//!   programming terms and custom dictionaries.
//! - [`words`]: word extraction per context (identifier splitting, URLs, paths, ...).
//! - [`suggest`]: weighted edit distance with keyboard proximity.
//! - [`grammar`]: Forge's own rule-based English grammar and style rules.

pub mod dictionary;
pub mod grammar;
pub mod hunspell;
pub mod suggest;
pub mod words;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use forge_codeintel::{ActionKind, ContextAction};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub use dictionary::{normalize, Dictionary};
pub use grammar::{Fix, RULES};
pub use words::{Context, Word};

/// A piece of a document to check: its UTF-16 offset in the document, its text and where it
/// comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub from: usize,
    pub text: String,
    pub kind: Context,
}

fn yes() -> bool {
    true
}

/// Which inspections run where (the `spelling.*` and `grammar.*` settings).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    #[serde(default = "yes")]
    pub spelling: bool,
    #[serde(default = "yes")]
    pub grammar: bool,
    #[serde(default = "yes")]
    pub comments: bool,
    #[serde(default = "yes")]
    pub strings: bool,
    #[serde(default = "yes")]
    pub identifiers: bool,
    /// Markdown and plain text files.
    #[serde(default = "yes")]
    pub text: bool,
    #[serde(default = "yes")]
    pub commit_messages: bool,
    #[serde(default)]
    pub file_names: bool,
    /// Grammar rule ids that are turned off.
    #[serde(default)]
    pub disabled_rules: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            spelling: true,
            grammar: true,
            comments: true,
            strings: true,
            identifiers: true,
            text: true,
            commit_messages: true,
            file_names: false,
            disabled_rules: Vec::new(),
        }
    }
}

impl Options {
    fn spell_checks(&self, context: Context) -> bool {
        self.spelling
            && match context {
                Context::Comment | Context::DocComment => self.comments,
                Context::String => self.strings,
                Context::Identifier => self.identifiers,
                Context::Text => self.text,
                Context::CommitMessage => self.commit_messages,
                Context::FileName => self.file_names,
            }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectRequest {
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub options: Options,
    /// The project-level and application-level dictionary words.
    #[serde(default)]
    pub words: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProblemKind {
    Typo,
    Grammar,
}

/// The identifier a misspelled word belongs to (for "Typo: Rename to...").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentifierSpan {
    pub from: usize,
    pub to: usize,
    pub text: String,
}

/// A problem with UTF-16 document offsets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub from: usize,
    pub to: usize,
    pub kind: ProblemKind,
    pub context: Context,
    pub message: String,
    /// The misspelled word (typos).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub word: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<IdentifierSpan>,
    /// The grammar rule id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    /// Grammar quick fixes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<FixSpec>,
}

/// A replacement quick fix with document offsets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixSpec {
    pub title: String,
    pub from: usize,
    pub to: usize,
    pub text: String,
}

/// The words of the user's dictionaries, normalized.
#[derive(Debug, Default)]
pub struct UserWords(HashSet<String>);

impl UserWords {
    pub fn new<S: AsRef<str>>(words: &[S]) -> UserWords {
        UserWords(
            words
                .iter()
                .map(|w| normalize(w.as_ref().trim()))
                .filter(|w| !w.is_empty())
                .collect(),
        )
    }
}

/// The dictionaries a check uses: the bundled English one, the programming terms and the
/// user's custom dictionaries.
#[derive(Debug, Clone)]
pub struct Speller {
    dictionaries: Vec<Arc<Dictionary>>,
}

impl Default for Speller {
    fn default() -> Self {
        Speller::new(Vec::new())
    }
}

impl Speller {
    pub fn new(custom: Vec<Arc<Dictionary>>) -> Speller {
        let mut dictionaries = vec![Dictionary::english(), Dictionary::programming()];
        dictionaries.extend(custom);
        Speller { dictionaries }
    }

    pub fn dictionaries(&self) -> &[Arc<Dictionary>] {
        &self.dictionaries
    }

    /// Whether `word` is spelled correctly (case is ignored).
    pub fn knows(&self, word: &str, user: &UserWords) -> bool {
        let w = normalize(word);
        let known = |w: &str| user.0.contains(w) || self.dictionaries.iter().any(|d| d.contains(w));
        known(&w)
            || w.strip_suffix("'s").is_some_and(known)
            || w.strip_suffix('\'').is_some_and(known)
    }

    /// Up to `limit` corrections for `word`, best first, in the word's case.
    pub fn suggest(&self, word: &str, user: &UserWords, limit: usize) -> Vec<String> {
        let lower = normalize(word);
        let chars: Vec<char> = lower.chars().collect();
        let n = chars.len();
        let max = suggest::max_distance(n);
        // Names and acronyms rank behind common words unless the typed word is capitalized.
        let name_penalty = if word.chars().any(char::is_uppercase) {
            0.0
        } else {
            0.4
        };
        let mut candidates = Vec::new();
        let mut buf: Vec<char> = Vec::new();
        let mut consider = |w: &str, proper: bool, candidates: &mut Vec<suggest::Candidate>| {
            buf.clear();
            buf.extend(w.chars());
            if let Some(cost) = suggest::distance(&chars, &buf, max) {
                if cost > 0.0 {
                    candidates.push(suggest::Candidate {
                        word: w.to_string(),
                        cost: cost + if proper { name_penalty } else { 0.0 },
                    });
                }
            }
        };
        for dict in &self.dictionaries {
            for len in n.saturating_sub(2)..=n + 2 {
                for (w, proper) in dict.words_of_len(len) {
                    consider(w, proper, &mut candidates);
                }
            }
        }
        for w in &user.0 {
            consider(w, false, &mut candidates);
        }
        // Phonetic replacements of the dictionaries (`REP f ph`, `REP alot a_lot`).
        for dict in &self.dictionaries {
            for (from, to) in &dict.replacements {
                for (at, _) in lower.match_indices(from.as_str()) {
                    let candidate = format!(
                        "{}{}{}",
                        &lower[..at],
                        to.replace('_', " "),
                        &lower[at + from.len()..]
                    );
                    if candidate.split(' ').all(|part| self.knows(part, user)) {
                        candidates.push(suggest::Candidate {
                            word: candidate,
                            cost: 0.7,
                        });
                    }
                }
            }
        }
        suggest::rank(&lower, candidates, limit)
            .into_iter()
            .map(|s| suggest::match_case(word, &s))
            .collect()
    }
}

/// Spelling and grammar problems of the segments.
pub fn inspect(speller: &Speller, request: &InspectRequest) -> Vec<Problem> {
    let user = UserWords::new(&request.words);
    let options = &request.options;
    let mut known: HashMap<String, bool> = HashMap::new();
    let mut out = Vec::new();
    for seg in &request.segments {
        if options.spell_checks(seg.kind) {
            for word in words::words(&seg.text, seg.kind) {
                let ok = *known
                    .entry(word.text.clone())
                    .or_insert_with(|| speller.knows(&word.text, &user));
                if ok {
                    continue;
                }
                out.push(Problem {
                    from: seg.from + word.from,
                    to: seg.from + word.to,
                    kind: ProblemKind::Typo,
                    context: seg.kind,
                    message: format!("Typo: In word '{}'", word.text),
                    identifier: word.identifier.map(|(from, to)| IdentifierSpan {
                        from: seg.from + from,
                        to: seg.from + to,
                        text: utf16_slice(&seg.text, from, to),
                    }),
                    word: Some(word.text),
                    rule: None,
                    fixes: Vec::new(),
                });
            }
        }
        let grammar_context = match seg.kind {
            Context::Comment | Context::DocComment => options.comments,
            Context::Text => options.text,
            Context::CommitMessage => options.commit_messages,
            _ => false,
        };
        if options.grammar && grammar_context {
            for p in grammar::check(&seg.text, seg.kind) {
                if options.disabled_rules.iter().any(|r| r == p.rule) {
                    continue;
                }
                out.push(Problem {
                    from: seg.from + p.from,
                    to: seg.from + p.to,
                    kind: ProblemKind::Grammar,
                    context: seg.kind,
                    message: p.message,
                    word: None,
                    identifier: None,
                    rule: Some(p.rule.to_string()),
                    fixes: p
                        .fixes
                        .into_iter()
                        .map(|f| FixSpec {
                            title: f.title,
                            from: seg.from + f.from,
                            to: seg.from + f.to,
                            text: f.text,
                        })
                        .collect(),
                });
            }
        }
    }
    out.sort_by_key(|p| (p.from, p.to));
    out
}

fn utf16_slice(text: &str, from: usize, to: usize) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    String::from_utf16_lossy(&units[from.min(units.len())..to.min(units.len())])
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionsRequest {
    /// The problems at the caret.
    pub problems: Vec<Problem>,
    #[serde(default)]
    pub words: Vec<String>,
    /// A project is open (offer the project-level dictionary).
    #[serde(default)]
    pub has_project: bool,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// UI actions of the entries (`registerContextActionHandler`).
pub const CHANGE_TO: &str = "SpellCheck.ChangeTo";
pub const RENAME_TO: &str = "SpellCheck.RenameTo";
pub const SAVE_TO: &str = "SpellCheck.SaveTo";
pub const REPLACE: &str = "SpellCheck.Replace";

fn entry(title: String, source: &str, action: &str, data: serde_json::Value) -> ContextAction {
    ContextAction {
        title,
        kind: ActionKind::QuickFix,
        source: source.into(),
        is_preferred: false,
        disabled: None,
        edit: None,
        command: None,
        needs_resolve: false,
        server: None,
        raw: None,
        action: Some(action.into()),
        children: Vec::new(),
        caret: None,
        data: Some(data),
    }
}

/// Alt+Enter entries for the problems at the caret.
pub fn context_actions(speller: &Speller, request: &ActionsRequest) -> Vec<ContextAction> {
    let user = UserWords::new(&request.words);
    let limit = request.limit.unwrap_or(10);
    let mut out = Vec::new();
    let mut saved: HashSet<String> = HashSet::new();
    for p in &request.problems {
        match p.kind {
            ProblemKind::Grammar => {
                for f in &p.fixes {
                    out.push(entry(
                        f.title.clone(),
                        "grammar",
                        REPLACE,
                        json!({ "from": f.from, "to": f.to, "text": f.text }),
                    ));
                }
            }
            ProblemKind::Typo => {
                let Some(word) = &p.word else { continue };
                let suggestions = speller.suggest(word, &user, limit);
                match &p.identifier {
                    Some(id) => {
                        // The identifier with the word replaced: `$usrName` → `$userName`.
                        let start = p.from - id.from;
                        let end = p.to - id.from;
                        let units: Vec<u16> = id.text.encode_utf16().collect();
                        let names: Vec<String> = suggestions
                            .iter()
                            .map(|s| {
                                let mut name =
                                    String::from_utf16_lossy(&units[..start.min(units.len())]);
                                name.push_str(s);
                                name.push_str(&String::from_utf16_lossy(
                                    &units[end.min(units.len())..],
                                ));
                                name
                            })
                            .collect();
                        let mut e = entry(
                            "Typo: Rename to...".into(),
                            "spelling",
                            RENAME_TO,
                            json!({ "from": id.from, "to": id.to, "word": word, "name": id.text, "suggestions": names }),
                        );
                        if names.is_empty() {
                            e.disabled = Some("No suggestions".into());
                        }
                        out.push(e);
                    }
                    None => {
                        let mut e = entry(
                            "Typo: Change to...".into(),
                            "spelling",
                            CHANGE_TO,
                            json!({ "from": p.from, "to": p.to, "word": word, "suggestions": suggestions }),
                        );
                        if suggestions.is_empty() {
                            e.disabled = Some("No suggestions".into());
                        }
                        out.push(e);
                    }
                }
                let lower = normalize(word);
                if !saved.insert(lower.clone()) {
                    continue;
                }
                if request.has_project {
                    out.push(entry(
                        format!("Save '{lower}' to project-level dictionary"),
                        "spelling",
                        SAVE_TO,
                        json!({ "word": lower, "level": "project" }),
                    ));
                }
                out.push(entry(
                    format!("Save '{lower}' to application-level dictionary"),
                    "spelling",
                    SAVE_TO,
                    json!({ "word": lower, "level": "application" }),
                ));
            }
        }
    }
    out
}

/// A custom dictionary as the Settings page lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub words: usize,
    pub builtin: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl DictionaryInfo {
    pub fn builtin(d: &Dictionary) -> DictionaryInfo {
        DictionaryInfo {
            name: d.name.clone(),
            path: None,
            words: d.len(),
            builtin: true,
            error: None,
        }
    }
}

/// The `.aff` file next to a `.dic` file.
pub fn aff_path_for(dic: &str) -> Option<String> {
    let stem = dic.strip_suffix(".dic")?;
    Some(format!("{stem}.aff"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(from: usize, text: &str, kind: Context) -> Segment {
        Segment {
            from,
            text: text.into(),
            kind,
        }
    }

    fn request(segments: Vec<Segment>) -> InspectRequest {
        InspectRequest {
            segments,
            ..Default::default()
        }
    }

    #[test]
    fn finds_typos_per_context() {
        let speller = Speller::default();
        let req = request(vec![
            seg(
                10,
                "// Recieve the ordr from Laravel's queue",
                Context::Comment,
            ),
            seg(60, "$usrNmae", Context::Identifier),
            seg(80, "'Invalid adress'", Context::String),
        ]);
        let problems = inspect(&speller, &req);
        let words: Vec<(&str, usize)> = problems
            .iter()
            .map(|p| (p.word.as_deref().unwrap(), p.from))
            .collect();
        assert_eq!(
            words,
            [
                ("Recieve", 13),
                ("ordr", 25),
                ("usr", 61),
                ("Nmae", 64),
                ("adress", 89)
            ]
        );
        assert_eq!(problems[0].message, "Typo: In word 'Recieve'");
        let id = problems[2].identifier.as_ref().unwrap();
        assert_eq!((id.from, id.to, id.text.as_str()), (61, 68, "usrNmae"));
    }

    #[test]
    fn options_and_user_words() {
        let speller = Speller::default();
        let mut req = request(vec![
            seg(0, "// the ordr", Context::Comment),
            seg(20, "$ordr", Context::Identifier),
        ]);
        req.options.identifiers = false;
        assert_eq!(inspect(&speller, &req).len(), 1);
        req.words = vec!["ORDR".into()];
        assert!(inspect(&speller, &req).is_empty());
        req.options.spelling = false;
        req.words.clear();
        assert!(inspect(&speller, &req).is_empty());
        // File names only when asked for.
        let req = request(vec![seg(0, "OrdrController.php", Context::FileName)]);
        assert!(inspect(&speller, &req).is_empty());
    }

    #[test]
    fn grammar_problems_with_fixes() {
        let speller = Speller::default();
        let req = request(vec![seg(
            100,
            "// Creates a order for the the user.",
            Context::Comment,
        )]);
        let problems = inspect(&speller, &req);
        let rules: Vec<_> = problems
            .iter()
            .map(|p| p.rule.as_deref().unwrap())
            .collect();
        assert_eq!(rules, ["a-an", "repeated-word"]);
        assert_eq!(problems[0].fixes[0].from, 111);
        let mut req = req;
        req.options.disabled_rules = vec!["a-an".into()];
        assert_eq!(inspect(&speller, &req).len(), 1);
        let req = request(vec![seg(0, "'Creates a order'", Context::String)]);
        assert!(inspect(&speller, &req).is_empty());
    }

    #[test]
    fn suggestions() {
        let speller = Speller::default();
        let user = UserWords::default();
        assert_eq!(speller.suggest("recieve", &user, 3)[0], "receive");
        assert_eq!(speller.suggest("Adress", &user, 3)[0], "Address");
        assert_eq!(speller.suggest("teh", &user, 3)[0], "the");
        assert_eq!(speller.suggest("ordr", &user, 3)[0], "order");
        assert!(speller
            .suggest("definately", &user, 5)
            .contains(&"definitely".to_string()));
        let user = UserWords::new(&["Zxqview"]);
        assert_eq!(speller.suggest("zxqviev", &user, 1), ["zxqview"]);
        assert!(speller.knows("Laravel's", &UserWords::default()));
    }

    #[test]
    fn context_action_entries() {
        let speller = Speller::default();
        let problems = inspect(
            &speller,
            &request(vec![
                seg(0, "// recieve", Context::Comment),
                seg(20, "$custmerName", Context::Identifier),
                seg(40, "// a order", Context::Comment),
            ]),
        );
        let titles = |problems: Vec<Problem>, has_project| {
            context_actions(
                &speller,
                &ActionsRequest {
                    problems,
                    has_project,
                    ..Default::default()
                },
            )
            .into_iter()
            .map(|a| a.title)
            .collect::<Vec<_>>()
        };
        assert_eq!(
            titles(vec![problems[0].clone()], true),
            [
                "Typo: Change to...",
                "Save 'recieve' to project-level dictionary",
                "Save 'recieve' to application-level dictionary"
            ]
        );
        assert_eq!(
            titles(vec![problems[1].clone()], false)[..2],
            [
                "Typo: Rename to...",
                "Save 'custmer' to application-level dictionary"
            ]
        );
        assert_eq!(
            titles(vec![problems[2].clone()], false),
            ["Replace with 'an'"]
        );

        let actions = context_actions(
            &speller,
            &ActionsRequest {
                problems: vec![problems[1].clone()],
                ..Default::default()
            },
        );
        let data = actions[0].data.as_ref().unwrap();
        assert!(
            data["suggestions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s == "customerName"),
            "{data}"
        );
        assert_eq!(
            (data["from"].as_u64(), data["to"].as_u64()),
            (Some(21), Some(32))
        );
        let change = context_actions(
            &speller,
            &ActionsRequest {
                problems: vec![problems[0].clone()],
                ..Default::default()
            },
        );
        assert_eq!(
            change[0].data.as_ref().unwrap()["suggestions"][0],
            "receive"
        );
        assert_eq!(change[0].action.as_deref(), Some(CHANGE_TO));
    }

    #[test]
    fn aff_next_to_dic() {
        assert_eq!(
            aff_path_for("/x/de_DE.dic").as_deref(),
            Some("/x/de_DE.aff")
        );
        assert_eq!(aff_path_for("/x/words.txt"), None);
    }
}
