//! Language injections, like PHPStorm's IntelliLang: SQL, HTML, CSS, JSON, RegExp,
//! JavaScript and XML inside the string literals of PHP and JavaScript/TypeScript files.
//!
//! - Where: `// language=SQL`, `/* language=JSON */`, `# language=HTML` and `/** @lang SQL */`
//!   comments before a literal, heredoc/nowdoc labels (`<<<SQL`), and the built-in
//!   [`rules`] (calls such as `PDO::query()`, `DB::select()`, `preg_match()`, `JSON.parse()`,
//!   tagged templates such as ``sql`…` ``, and strings that start with a SQL statement or an
//!   HTML tag). Rules can be turned off (`editor.injections.disabled`) and single places
//!   un-injected (`editor.injections.uninjected`, entries `path#key`).
//! - Literals joined with `.` / `+` form one injection; `$var` / `${expr}` interpolations
//!   are left out of the injected ranges.
//! - Fragments: the value of a literal for Edit Fragment, with an offset map, and
//!   [`escape`] for text typed into it; Run SQL uses [`Injection::text`].
//! - Intentions for Show Context Actions (Alt+Enter): "Inject language or reference",
//!   "Un-inject Language" and "Edit <language> Fragment".
//!
//! All offsets of the public API are UTF-16 code units, like CodeMirror's.

pub mod escape;
pub mod languages;
pub mod lexer;
pub mod rules;

#[cfg(test)]
mod tests;

use forge_codeintel::{position_at, ActionKind, ContextAction, Range, TextEdit, WorkspaceEdit};
use serde::{Deserialize, Serialize};

pub use escape::{escape, unescape};
pub use languages::{Language, LANGUAGES};
pub use lexer::{Host, LiteralKind};
pub use rules::{rule_infos, RuleInfo, Source, RULES};

use lexer::{Lexed, Literal};
use rules::Found;

/// The settings of a request: disabled rules and un-injected places.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InjectionSettings {
    /// Ids of the built-in rules that are turned off.
    pub disabled: Vec<String>,
    /// `path#key` of places un-injected with "Un-inject Language".
    pub uninjected: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindRequest {
    pub text: String,
    pub path: String,
    #[serde(default, flatten)]
    pub settings: InjectionSettings,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OffsetRequest {
    pub text: String,
    pub path: String,
    pub offset: usize,
    #[serde(default, flatten)]
    pub settings: InjectionSettings,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InjectRequest {
    pub text: String,
    pub path: String,
    pub offset: usize,
    /// Language id or name.
    pub language: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub from: usize,
    pub to: usize,
}

/// A literal of an injection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiteralInfo {
    pub from: usize,
    pub to: usize,
    pub content_from: usize,
    pub content_to: usize,
    pub kind: LiteralKind,
}

/// An injected language in a string literal (or in literals joined with `.` / `+`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Injection {
    /// Editor language id (`sql`, `html`, `regexp`, ...).
    pub language: String,
    /// Display name (`SQL`, `RegExp`).
    pub label: String,
    pub source: Source,
    /// The built-in rule that injected it.
    pub rule: Option<String>,
    /// From the first literal's start to the last literal's end.
    pub from: usize,
    pub to: usize,
    /// The injected text, without quotes, delimiters and interpolations.
    pub ranges: Vec<Span>,
    pub literals: Vec<LiteralInfo>,
    /// The `language=` comment.
    pub comment: Option<Span>,
    /// Identifies the place for "Un-inject Language" (a hash of the literals).
    pub key: String,
    /// The injected value: the literals unescaped and joined (for Run SQL).
    pub text: String,
}

/// The value of one literal of an injection, for Edit Fragment.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fragment {
    pub language: String,
    pub label: String,
    pub kind: LiteralKind,
    /// The source range the fragment stands for.
    pub from: usize,
    pub to: usize,
    pub text: String,
    /// For each UTF-16 unit of `text` and its end: the source offset.
    pub map: Vec<usize>,
}

/// A replacement of `from..to` by `text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Edit {
    pub from: usize,
    pub to: usize,
    pub text: String,
}

/// Byte <-> UTF-16 offsets of one text.
struct Utf16 {
    /// UTF-16 offset of every byte (and the end); `None` for ASCII texts.
    prefix: Option<Vec<usize>>,
    len: usize,
}

impl Utf16 {
    fn new(text: &str) -> Self {
        if text.is_ascii() {
            return Utf16 {
                prefix: None,
                len: text.len(),
            };
        }
        let mut prefix = Vec::with_capacity(text.len() + 1);
        let mut cur = 0;
        for ch in text.chars() {
            for _ in 0..ch.len_utf8() {
                prefix.push(cur);
            }
            cur += ch.len_utf16();
        }
        prefix.push(cur);
        Utf16 {
            prefix: Some(prefix),
            len: text.len(),
        }
    }

    fn to16(&self, byte: usize) -> usize {
        let byte = byte.min(self.len);
        self.prefix.as_ref().map_or(byte, |p| p[byte])
    }

    fn to8(&self, u16: usize) -> usize {
        match &self.prefix {
            None => u16.min(self.len),
            Some(p) => p.partition_point(|&x| x < u16).min(self.len),
        }
    }
}

/// FNV-1a of a literal's source, the stable part of an un-injected place.
fn place_key(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// The analysis of one source.
struct Analysis<'a> {
    src: &'a str,
    path: &'a str,
    lexed: Lexed,
    found: Vec<Found>,
    u: Utf16,
}

impl<'a> Analysis<'a> {
    fn new(src: &'a str, path: &'a str, settings: &InjectionSettings) -> Option<Self> {
        let host = Host::of_path(path)?;
        let lexed = lexer::lex(src, host);
        let mut found = rules::detect(src, host, &lexed, &settings.disabled);
        let mut a = Analysis {
            src,
            path,
            lexed,
            found: Vec::new(),
            u: Utf16::new(src),
        };
        found.retain(|f| {
            f.source == Source::Comment
                || !settings
                    .uninjected
                    .iter()
                    .any(|e| e == &format!("{}#{}", path, a.key_of(f)))
        });
        a.found = found;
        Some(a)
    }

    fn lit(&self, i: usize) -> &Literal {
        &self.lexed.literals[i]
    }

    fn key_of(&self, f: &Found) -> String {
        let first = self.lit(f.literals[0]);
        let last = self.lit(*f.literals.last().unwrap_or(&f.literals[0]));
        place_key(&self.src[first.start..last.end])
    }

    fn injection(&self, f: &Found) -> Injection {
        let u = &self.u;
        let first = self.lit(f.literals[0]);
        let last = self.lit(*f.literals.last().unwrap_or(&f.literals[0]));
        let text = f
            .literals
            .iter()
            .map(|&i| {
                let lit = self.lit(i);
                let (s, e) = rules::fragment_range(self.src, lit, f.language);
                unescape(lit.kind, self.src, s, e).0
            })
            .collect::<String>();
        Injection {
            language: f.language.id.into(),
            label: f.language.label.into(),
            source: f.source,
            rule: f.rule.map(Into::into),
            from: u.to16(first.start),
            to: u.to16(last.end),
            ranges: f
                .ranges
                .iter()
                .map(|&(s, e)| Span {
                    from: u.to16(s),
                    to: u.to16(e),
                })
                .collect(),
            literals: f
                .literals
                .iter()
                .map(|&i| {
                    let l = self.lit(i);
                    LiteralInfo {
                        from: u.to16(l.start),
                        to: u.to16(l.end),
                        content_from: u.to16(l.content_start),
                        content_to: u.to16(l.content_end),
                        kind: l.kind,
                    }
                })
                .collect(),
            comment: f.comment.map(|c| Span {
                from: u.to16(c.start),
                to: u.to16(c.end),
            }),
            key: self.key_of(f),
            text,
        }
    }

    /// The literal around a byte offset: inside its token (`inclusive`: also right at the
    /// content's ends, for fragment anchors).
    fn literal_at(&self, at: usize, inclusive: bool) -> Option<usize> {
        let lits = &self.lexed.literals;
        // The innermost literal (template expressions nest literals).
        lits.iter()
            .enumerate()
            .filter(|(_, l)| {
                if inclusive {
                    l.content_start <= at && at <= l.content_end
                } else {
                    l.start < at && at < l.end
                }
            })
            .max_by_key(|(_, l)| l.start)
            .map(|(i, _)| i)
    }

    fn found_with(&self, lit: usize) -> Option<&Found> {
        self.found.iter().find(|f| f.literals.contains(&lit))
    }
}

/// Every injection of a file (none for files that are not PHP or JavaScript/TypeScript).
pub fn find(req: &FindRequest) -> Vec<Injection> {
    let Some(a) = Analysis::new(&req.text, &req.path, &req.settings) else {
        return Vec::new();
    };
    a.found.iter().map(|f| a.injection(f)).collect()
}

/// The injection at an offset.
pub fn injection_at(req: &OffsetRequest) -> Option<Injection> {
    let a = Analysis::new(&req.text, &req.path, &req.settings)?;
    let at = a.u.to8(req.offset);
    let lit = a.literal_at(at, false).or_else(|| a.literal_at(at, true))?;
    a.found_with(lit).map(|f| a.injection(f))
}

/// The fragment of the injected literal at an offset (its content, inclusive of its ends).
pub fn fragment(req: &OffsetRequest) -> Option<Fragment> {
    let a = Analysis::new(&req.text, &req.path, &req.settings)?;
    let at = a.u.to8(req.offset);
    let i = a.literal_at(at, true).or_else(|| a.literal_at(at, false))?;
    let f = a.found_with(i)?;
    let lit = a.lit(i);
    let (s, e) = rules::fragment_range(a.src, lit, f.language);
    let (text, bytes) = unescape(lit.kind, a.src, s, e);
    let mut map = Vec::with_capacity(text.len() + 1);
    for (k, ch) in text.char_indices() {
        for _ in 0..ch.len_utf16() {
            map.push(a.u.to16(bytes[k]));
        }
    }
    map.push(a.u.to16(e));
    Some(Fragment {
        language: f.language.id.into(),
        label: f.language.label.into(),
        kind: lit.kind,
        from: a.u.to16(s),
        to: a.u.to16(e),
        text,
        map,
    })
}

/// Start of the line containing `i`.
fn line_start(src: &str, i: usize) -> usize {
    src[..i].rfind('\n').map_or(0, |p| p + 1)
}

/// "Inject language or reference": a `// language=…` comment before the literal at the
/// offset, or `/* language=… */` right before it when other code precedes it on its line.
/// An existing `language=` comment is replaced.
pub fn inject(req: &InjectRequest) -> Option<Edit> {
    let lang = languages::language(&req.language).or_else(|| languages::by_id(&req.language))?;
    let a = Analysis::new(&req.text, &req.path, &InjectionSettings::default())?;
    let at = a.u.to8(req.offset);
    let i = a.literal_at(at, false).or_else(|| a.literal_at(at, true))?;
    let src = a.src;
    // The first literal of its group.
    let first = a
        .found
        .iter()
        .find(|f| f.literals.contains(&i))
        .map_or(i, |f| f.literals[0]);
    let lit = a.lit(first);
    if let Some(c) = a
        .found
        .iter()
        .find(|f| f.literals.contains(&i))
        .and_then(|f| f.comment)
    {
        let old = &src[c.start..c.end];
        let text = if old.starts_with("/*") {
            format!("/* language={} */", lang.label)
        } else if old.starts_with('#') {
            format!("# language={}", lang.label)
        } else {
            format!("// language={}", lang.label)
        };
        return Some(Edit {
            from: a.u.to16(c.start),
            to: a.u.to16(c.end),
            text,
        });
    }
    let ls = line_start(src, lit.start);
    let before = &src[ls..lit.start];
    let busy = a
        .lexed
        .literals
        .iter()
        .any(|l| l.start >= ls && l.start < lit.start)
        || a.lexed
            .comments
            .iter()
            .any(|c| c.end > ls && c.start < lit.start)
        || before.contains([';', '{', '}'])
        || before.contains("<?")
        || before.contains("?>");
    if busy {
        return Some(Edit {
            from: a.u.to16(lit.start),
            to: a.u.to16(lit.start),
            text: format!("/* language={} */ ", lang.label),
        });
    }
    let indent: String = before
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    Some(Edit {
        from: a.u.to16(ls),
        to: a.u.to16(ls),
        text: format!("{indent}// language={}\n", lang.label),
    })
}

/// "Un-inject Language" of a comment injection: removes the comment (its whole line when it
/// is alone there).
fn remove_comment(src: &str, c: lexer::Comment) -> (usize, usize) {
    let ls = line_start(src, c.start);
    let le = src[c.end..].find('\n').map_or(src.len(), |p| c.end + p);
    if src[ls..c.start].trim().is_empty() && src[c.end..le].trim().is_empty() {
        return (ls, (le + 1).min(src.len()));
    }
    let mut end = c.end;
    while src.as_bytes().get(end) == Some(&b' ') {
        end += 1;
    }
    (c.start, end)
}

/// The injection intentions at the caret, for Show Context Actions.
pub fn context_actions(req: &OffsetRequest) -> Vec<ContextAction> {
    let Some(a) = Analysis::new(&req.text, &req.path, &req.settings) else {
        return Vec::new();
    };
    let at = a.u.to8(req.offset);
    let Some(i) = a.literal_at(at, false) else {
        return Vec::new();
    };
    let intention =
        |title: String, action: Option<&str>, edit: Option<WorkspaceEdit>| ContextAction {
            title,
            kind: ActionKind::Intention,
            source: "injections".into(),
            is_preferred: false,
            disabled: None,
            edit,
            command: None,
            needs_resolve: false,
            server: None,
            raw: None,
            action: action.map(Into::into),
            children: vec![],
            caret: None,
            data: None,
        };
    let Some(f) = a.found_with(i) else {
        return vec![intention(
            "Inject language or reference".into(),
            Some("InjectLanguageAction"),
            None,
        )];
    };
    let mut out = vec![intention(
        format!("Edit {} Fragment", f.language.label),
        Some("EditInjectedFragment"),
        None,
    )];
    match f.comment {
        Some(c) => {
            let (s, e) = remove_comment(a.src, c);
            let mut edit = WorkspaceEdit::default();
            edit.add_edits(
                a.path.to_string(),
                vec![TextEdit {
                    range: Range::new(
                        position_at(a.src, a.u.to16(s)),
                        position_at(a.src, a.u.to16(e)),
                    ),
                    new_text: String::new(),
                }],
            );
            out.push(intention("Un-inject Language".into(), None, Some(edit)));
        }
        None => out.push(intention(
            "Un-inject Language".into(),
            Some("UnInjectLanguageAction"),
            None,
        )),
    }
    out
}

/// The injectable languages, for the "Inject language or reference" chooser.
pub fn languages() -> Vec<Language> {
    LANGUAGES.to_vec()
}
