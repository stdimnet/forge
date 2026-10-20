//! Find Usages results: what kind of usage each reference is (PHPStorm's usage types) and
//! the tree the Find tool window and the Show Usages popup render, grouped by usage type and
//! file. Classification looks at the line around the reference, so it works for every
//! language server and for the PHP index alike.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::edit::Range;

/// PHPStorm's usage types, in the order the Find tool window lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageKind {
    Import,
    TraitUse,
    Extends,
    NewInstance,
    MethodCall,
    FunctionCall,
    StaticAccess,
    Write,
    Read,
    TypeHint,
    Instanceof,
    Attribute,
    Doc,
    Comment,
    StringLiteral,
    /// A member accessed on something the index cannot type (`$x->save()`).
    Dynamic,
    Unclassified,
}

impl UsageKind {
    /// The group title in the Find tool window.
    pub fn label(self) -> &'static str {
        match self {
            UsageKind::Import => "Usage in import",
            UsageKind::TraitUse => "Usage in trait use",
            UsageKind::Extends => "Usage in extends/implements clause",
            UsageKind::NewInstance => "New instance creation",
            UsageKind::MethodCall => "Method call",
            UsageKind::FunctionCall => "Function call",
            UsageKind::StaticAccess => "Static member access",
            UsageKind::Write => "Value write",
            UsageKind::Read => "Value read",
            UsageKind::TypeHint => "Usage in type declaration",
            UsageKind::Instanceof => "Usage in instanceof",
            UsageKind::Attribute => "Usage in attribute",
            UsageKind::Doc => "Usage in PHPDoc",
            UsageKind::Comment => "Usage in comments",
            UsageKind::StringLiteral => "Usage in string literals",
            UsageKind::Dynamic => "Dynamic usages",
            UsageKind::Unclassified => "Unclassified",
        }
    }
}

/// Byte index of UTF-16 offset `units` in `s` (clamped).
fn byte_at(s: &str, units: u32) -> usize {
    let mut n = 0u32;
    for (i, c) in s.char_indices() {
        if n >= units {
            return i;
        }
        n += c.len_utf16() as u32;
    }
    s.len()
}

fn utf16_len(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

fn ends_with_word(s: &str, word: &str) -> bool {
    s.strip_suffix(word).is_some_and(|rest| {
        !rest
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$')
    })
}

fn has_word(s: &str, word: &str) -> bool {
    s.match_indices(word).any(|(i, _)| {
        let before = s[..i].chars().next_back();
        let after = s[i + word.len()..].chars().next();
        let boundary = |c: Option<char>| !c.is_some_and(|c| c.is_alphanumeric() || c == '_');
        boundary(before) && boundary(after)
    })
}

/// Whether `before` ends inside a string literal (an odd number of unescaped quotes).
fn in_string(before: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in before.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match (quote, c) {
            (Some(_), '\\') => escaped = true,
            (Some(q), c) if c == q => quote = None,
            (None, '\'' | '"' | '`') => quote = Some(c),
            _ => {}
        }
    }
    quote.is_some()
}

/// An assignment right after a reference (`= …`, `+= …`, `++`), not a comparison or `=>`.
fn is_assignment(after: &str) -> bool {
    let a = after.trim_start();
    if a.starts_with("++") || a.starts_with("--") {
        return true;
    }
    let ops = ["??=", ".=", "+=", "-=", "*=", "/=", "%=", "|=", "&=", "^=", "**="];
    if ops.iter().any(|op| a.starts_with(op)) {
        return true;
    }
    a.starts_with('=') && !a.starts_with("==") && !a.starts_with("=>")
}

/// The usage type of the reference at UTF-16 columns `start..end` of `line`.
pub fn classify_usage(line: &str, start: u32, end: u32) -> UsageKind {
    let s = byte_at(line, start);
    let e = byte_at(line, end.max(start)).max(s);
    let before = &line[..s];
    let word = &line[s..e];
    let after = &line[e..];
    let lead = before.trim_start();
    let trimmed_line = line.trim_start();
    let b = before.trim_end();
    let a = after.trim_start();

    // Comments and docblocks.
    if trimmed_line.starts_with("/**") || trimmed_line.starts_with('*') {
        return UsageKind::Doc;
    }
    if lead.starts_with("//")
        || (lead.starts_with('#') && !lead.starts_with("#["))
        || lead.starts_with("/*")
        || (!in_string(before) && before.contains("//") && !before.contains("://"))
    {
        return UsageKind::Comment;
    }
    if in_string(before) {
        return UsageKind::StringLiteral;
    }

    // `use App\Models\User;` / `import { x } from` at file level; `use Trait;` in a class.
    if trimmed_line.starts_with("use ") {
        return if line.len() == trimmed_line.len() {
            UsageKind::Import
        } else {
            UsageKind::TraitUse
        };
    }
    if trimmed_line.starts_with("import ") || (trimmed_line.starts_with("export ") && line.contains(" from ")) {
        return UsageKind::Import;
    }
    if b.ends_with("#[") || (b.ends_with(',') && lead.starts_with("#[")) {
        return UsageKind::Attribute;
    }
    if ends_with_word(b, "new") {
        return UsageKind::NewInstance;
    }
    if ends_with_word(b, "instanceof") {
        return UsageKind::Instanceof;
    }
    if has_word(before, "extends") || has_word(before, "implements") {
        return UsageKind::Extends;
    }

    let member = b.ends_with("->") || b.ends_with("::") || (b.ends_with('.') && !b.ends_with(".."));
    if member && !word.is_empty() {
        if a.starts_with('(') {
            return UsageKind::MethodCall;
        }
        if is_assignment(after) {
            return UsageKind::Write;
        }
        return if b.ends_with("::") && !word.starts_with('$') {
            UsageKind::StaticAccess
        } else {
            UsageKind::Read
        };
    }
    if a.starts_with("::") {
        return UsageKind::StaticAccess;
    }
    if word.starts_with('$') {
        return if is_assignment(after) {
            UsageKind::Write
        } else {
            UsageKind::Read
        };
    }
    if a.starts_with('(') {
        return UsageKind::FunctionCall;
    }
    // Parameter, property and return types: `User $user`, `?User`, `): User`, `A|B`.
    if a.starts_with('$')
        || a.starts_with("&$")
        || a.starts_with("...$")
        || (b.ends_with(':') && !b.ends_with("::"))
        || b.ends_with('?')
        || b.ends_with('|')
        || (a.starts_with('|') && !a.starts_with("||"))
    {
        return UsageKind::TypeHint;
    }
    UsageKind::Unclassified
}

/// A reference as language servers and indexes report it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLocation {
    pub path: String,
    pub range: Range,
    /// Set by sources that already know it (the PHP index marks dynamic usages).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<UsageKind>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GroupRequest {
    pub usages: Vec<UsageLocation>,
    /// The declaration itself; Find Usages does not list it.
    pub declaration: Option<UsageLocation>,
    /// Group by usage type ("Group by Usage Type" toggle); otherwise only by file.
    pub group_by_type: bool,
}

/// One usage: its line (leading whitespace removed) with the reference marked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRow {
    /// 0-based line and UTF-16 column of the reference.
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub text: String,
    /// UTF-16 offsets into `text`.
    pub ranges: Vec<[u32; 2]>,
    pub kind: UsageKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageFile {
    pub path: String,
    pub usages: Vec<UsageRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageGroup {
    /// `None` when not grouping by usage type.
    pub kind: Option<UsageKind>,
    pub label: String,
    pub count: usize,
    pub files: Vec<UsageFile>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTree {
    pub total: usize,
    pub file_count: usize,
    pub groups: Vec<UsageGroup>,
}

/// Build the usage tree. `source` returns a file's content (an open editor's buffer or the
/// file on disk); usages in files it cannot read keep an empty line.
pub fn group_usages<'a>(
    req: &GroupRequest,
    mut source: impl FnMut(&str) -> Option<&'a str>,
) -> UsageTree {
    let mut seen = std::collections::HashSet::new();
    let mut rows: Vec<(String, UsageRow)> = Vec::new();
    for u in &req.usages {
        if req
            .declaration
            .as_ref()
            .is_some_and(|d| d.path == u.path && d.range.start == u.range.start)
        {
            continue;
        }
        if !seen.insert((u.path.clone(), u.range.start)) {
            continue;
        }
        let line = source(&u.path)
            .and_then(|t| t.split('\n').nth(u.range.start.line as usize))
            .map(|l| l.strip_suffix('\r').unwrap_or(l))
            .unwrap_or("");
        let end_char = if u.range.end.line == u.range.start.line {
            u.range.end.character
        } else {
            utf16_len(line)
        };
        let kind = u
            .kind
            .unwrap_or_else(|| classify_usage(line, u.range.start.character, end_char));
        let lead = utf16_len(line) - utf16_len(line.trim_start());
        rows.push((
            u.path.clone(),
            UsageRow {
                line: u.range.start.line,
                column: u.range.start.character,
                end_line: u.range.end.line,
                end_column: u.range.end.character,
                text: line.trim().to_string(),
                ranges: vec![[
                    u.range.start.character.saturating_sub(lead),
                    end_char.saturating_sub(lead),
                ]],
                kind,
            },
        ));
    }
    let total = rows.len();
    let file_count = rows
        .iter()
        .map(|(p, _)| p.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();

    let mut groups: BTreeMap<Option<UsageKind>, BTreeMap<String, Vec<UsageRow>>> = BTreeMap::new();
    for (path, row) in rows {
        let key = req.group_by_type.then_some(row.kind);
        groups
            .entry(key)
            .or_default()
            .entry(path)
            .or_default()
            .push(row);
    }
    UsageTree {
        total,
        file_count,
        groups: groups
            .into_iter()
            .map(|(kind, files)| {
                let files: Vec<UsageFile> = files
                    .into_iter()
                    .map(|(path, mut usages)| {
                        usages.sort_by_key(|u| (u.line, u.column));
                        UsageFile { path, usages }
                    })
                    .collect();
                UsageGroup {
                    kind,
                    label: kind.map(UsageKind::label).unwrap_or_default().to_string(),
                    count: files.iter().map(|f| f.usages.len()).sum(),
                    files,
                }
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(line: &str, word: &str) -> UsageKind {
        let at = line.find(word).expect("word in line");
        let start = line[..at].encode_utf16().count() as u32;
        classify_usage(line, start, start + word.encode_utf16().count() as u32)
    }

    #[test]
    fn classifies_php_usages() {
        assert_eq!(kind("use App\\Models\\User;", "User"), UsageKind::Import);
        assert_eq!(kind("    use HasFactory, Notifiable;", "Notifiable"), UsageKind::TraitUse);
        assert_eq!(kind("class Admin extends User implements Auth", "User"), UsageKind::Extends);
        assert_eq!(kind("class Admin extends User implements Auth", "Auth"), UsageKind::Extends);
        assert_eq!(kind("        $u = new User();", "User"), UsageKind::NewInstance);
        assert_eq!(kind("        $user->orders()->get();", "orders"), UsageKind::MethodCall);
        assert_eq!(kind("        return $this->orders", "orders"), UsageKind::Read);
        assert_eq!(kind("        $this->total = 5;", "total"), UsageKind::Write);
        assert_eq!(kind("        $this->total += 5;", "total"), UsageKind::Write);
        assert_eq!(kind("        if ($this->total == 5)", "total"), UsageKind::Read);
        assert_eq!(kind("        User::find(1);", "User"), UsageKind::StaticAccess);
        assert_eq!(kind("        return $this->hasMany(Order::class);", "Order"), UsageKind::StaticAccess);
        assert_eq!(kind("        OrderStatus::Paid", "Paid"), UsageKind::StaticAccess);
        assert_eq!(kind("    public function show(Request $r, Order $order): JsonResponse", "Order"), UsageKind::TypeHint);
        assert_eq!(kind("    public function show(Request $r, Order $order): JsonResponse", "JsonResponse"), UsageKind::TypeHint);
        assert_eq!(kind("    private ?User $owner;", "User"), UsageKind::TypeHint);
        assert_eq!(kind("        if ($x instanceof User) {", "User"), UsageKind::Instanceof);
        assert_eq!(kind("    #[Route('/orders')]", "Route"), UsageKind::Attribute);
        assert_eq!(kind(" * @return User the owner", "User"), UsageKind::Doc);
        assert_eq!(kind("    // the User model", "User"), UsageKind::Comment);
        assert_eq!(kind("        $x = 'User';", "User"), UsageKind::StringLiteral);
        assert_eq!(kind("        $user = $request->user();", "$user"), UsageKind::Write);
        assert_eq!(kind("        return $user;", "$user"), UsageKind::Read);
        assert_eq!(kind("        response()->json([]);", "response"), UsageKind::FunctionCall);
        assert_eq!(kind("import { ref } from 'vue';", "ref"), UsageKind::Import);
    }

    fn loc(path: &str, line: u32, start: u32, end: u32) -> UsageLocation {
        UsageLocation {
            path: path.into(),
            range: Range::on_line(line, start, end),
            kind: None,
        }
    }

    #[test]
    fn groups_by_type_and_file_without_the_declaration() {
        let user = "<?php\nclass User\n{\n}\n";
        let ctrl = "<?php\nuse App\\Models\\User;\n\nfunction a(User $u) {\n    return new User();\n}\n";
        let req = GroupRequest {
            usages: vec![
                loc("app/Http/Ctrl.php", 4, 15, 19),
                loc("app/Models/User.php", 1, 6, 10),
                loc("app/Http/Ctrl.php", 1, 15, 19),
                loc("app/Http/Ctrl.php", 3, 11, 15),
                // Duplicate from a second server.
                loc("app/Http/Ctrl.php", 3, 11, 15),
            ],
            declaration: Some(loc("app/Models/User.php", 1, 6, 10)),
            group_by_type: true,
        };
        let tree = group_usages(&req, |p| match p {
            "app/Models/User.php" => Some(user),
            "app/Http/Ctrl.php" => Some(ctrl),
            _ => None,
        });
        assert_eq!(tree.total, 3);
        assert_eq!(tree.file_count, 1);
        let labels: Vec<&str> = tree.groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["Usage in import", "New instance creation", "Usage in type declaration"]
        );
        let row = &tree.groups[1].files[0].usages[0];
        assert_eq!(row.text, "return new User();");
        assert_eq!(row.ranges, vec![[11, 15]]);
        assert_eq!(&row.text[11..15], "User");

        let flat = group_usages(
            &GroupRequest {
                group_by_type: false,
                ..req
            },
            |_| Some(ctrl),
        );
        assert_eq!(flat.groups.len(), 1);
        assert_eq!(flat.groups[0].kind, None);
        let lines: Vec<u32> = flat.groups[0].files[0].usages.iter().map(|u| u.line).collect();
        assert_eq!(lines, vec![1, 3, 4], "sorted by position");
    }

    #[test]
    fn known_kinds_and_unreadable_files_are_kept() {
        let req = GroupRequest {
            usages: vec![UsageLocation {
                kind: Some(UsageKind::Dynamic),
                ..loc("gone.php", 0, 0, 4)
            }],
            declaration: None,
            group_by_type: true,
        };
        let tree = group_usages(&req, |_| None);
        assert_eq!(tree.groups[0].label, "Dynamic usages");
        assert_eq!(tree.groups[0].files[0].usages[0].text, "");
    }
}
