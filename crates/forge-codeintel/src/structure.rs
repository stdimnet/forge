//! The code structure the editor shows around the caret, like PHPStorm:
//!
//! - Sticky Lines: the headers of the scopes that enclose the first visible line
//!   (namespaces, classes, functions and methods, `if` / `for` / `try` blocks, CSS rules,
//!   HTML elements, Lua functions and blocks) stay at the top of the editor.
//! - Breadcrumbs: the declarations around the caret (`User › orders()`, `div.card`,
//!   `@media … › .card`); every crumb lists its siblings.
//! - Custom folding regions: `// region Name` … `// endregion`, `#region` / `#endregion`,
//!   `<!-- region -->`, `-- region` (Lua) and NetBeans' `// <editor-fold desc="…">`.
//!
//! [`editor_structure`] scans a document once per version (no parser needed: braces, tags
//! and Lua's block keywords outside strings and comments); the UI keeps the answer per
//! document and filters it per scroll position or caret.
//!
//! Lines are 0-based; columns are UTF-16 code units like LSP positions.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::docs::{byte_col, header_start, lines_of};

/// What a scope is; decides its icon and whether it sticks or shows as a crumb.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScopeKind {
    Namespace,
    Class,
    Interface,
    Trait,
    Enum,
    Function,
    Method,
    /// An anonymous function or callback.
    Closure,
    /// `if`, `else`, `for`, `foreach`, `while`, `switch`, `try`, `catch`, `match`, ...
    Control,
    /// A named object literal (`methods: {`, `const config = {`).
    Object,
    /// A CSS rule or at-rule.
    Rule,
    /// An HTML / XML element.
    Tag,
}

impl ScopeKind {
    /// Shown as a breadcrumb (declarations, rules and tags, not control statements).
    pub fn is_crumb(self) -> bool {
        self != ScopeKind::Control
    }
}

/// One scope of a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub kind: ScopeKind,
    /// Crumb text: `User`, `orders()`, `if`, `.card`, `div.container`.
    pub name: String,
    /// First line of the header (the sticky line).
    pub line: u32,
    /// Where navigation puts the caret: the name in the header, else its first character.
    pub column: u32,
    /// Where the scope starts on `line` (first non-blank character, or `<` of a tag).
    pub start_column: u32,
    /// The line of the `{` (or the end of an opening tag); the body starts after it.
    pub open_line: u32,
    /// The line of the closing `}`, `end` or end tag (the last line when unclosed).
    pub end_line: u32,
    /// The column after the closing token.
    pub end_column: u32,
    /// Index of the enclosing scope in [`EditorStructure::scopes`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<u32>,
    /// Sticks to the top of the editor (multi-line scopes).
    pub sticky: bool,
    /// Shown in the breadcrumbs.
    pub crumb: bool,
}

/// A custom folding region.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    /// The region's name (`desc` of an `editor-fold`); empty when it has none.
    pub name: String,
    /// Line of the start marker.
    pub line: u32,
    /// Column where the start marker's comment begins (the fold starts there).
    pub column: u32,
    /// Line of the end marker.
    pub end_line: u32,
    /// Column after the end marker line's text.
    pub end_column: u32,
    /// `defaultstate="collapsed"`: folded when the file opens.
    #[serde(default)]
    pub collapsed: bool,
}

/// Everything [`editor_structure`] finds in a document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorStructure {
    /// Ordered by start position; parents come before their children.
    pub scopes: Vec<Scope>,
    pub regions: Vec<Region>,
}

/// How a language delimits its scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Php,
    Script,
    Css,
    /// Other brace languages (Java, Go, Rust, C, ...).
    Braces,
    Markup,
    Lua,
    /// No scopes (plain text, YAML, Markdown, ...); regions only.
    None,
}

/// The family of a file type id (`php`, `typescript`, `vue`, ...) or LSP language id.
fn family(language: &str) -> Family {
    match language {
        "php" => Family::Php,
        "javascript" | "jsx" | "typescript" | "tsx" | "javascriptreact" | "typescriptreact" => {
            Family::Script
        }
        "css" | "scss" | "less" => Family::Css,
        "html" | "xml" | "vue" | "svg" | "blade" | "twig" | "smarty" | "latte" | "jinja"
        | "handlebars" | "gotemplate" | "xhtml" => Family::Markup,
        "lua" => Family::Lua,
        "java" | "go" | "rust" | "c" | "cpp" | "csharp" | "kotlin" | "scala" | "swift" | "dart"
        | "groovy" | "objc" => Family::Braces,
        _ => Family::None,
    }
}

/// Whether a language has region comments at all (Markdown's `# region` is a heading).
fn has_region_comments(language: &str) -> bool {
    !matches!(language, "markdown" | "plaintext" | "text" | "json")
}

/// The scopes and folding regions of `text` in `language` (a Forge file type id).
pub fn editor_structure(text: &str, language: &str) -> EditorStructure {
    let lines = lines_of(text);
    let fam = family(language);
    let cs: Vec<char> = text.chars().collect();
    let mut scopes = Vec::new();
    match fam {
        Family::Php | Family::Script | Family::Css | Family::Braces => {
            let braces = scan_braces(&cs, 0, cs.len(), Pos::default(), fam);
            brace_scopes(&braces, &lines, fam, None, &mut scopes);
        }
        Family::Markup => markup_scopes(&cs, &lines, language, &mut scopes),
        Family::Lua => lua_scopes(&cs, &lines, &mut scopes),
        Family::None => {}
    }
    let scopes = compact(scopes);
    let regions = if has_region_comments(language) {
        regions(&lines, language)
    } else {
        Vec::new()
    };
    EditorStructure { scopes, regions }
}

// ---- Positions ----

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Pos {
    line: usize,
    /// UTF-16 column.
    col: usize,
}

impl Pos {
    fn advance(&mut self, c: char) {
        if c == '\n' {
            self.line += 1;
            self.col = 0;
        } else {
            self.col += c.len_utf16();
        }
    }
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// UTF-16 column of the first non-blank character of a line.
fn indent_col(line: &str) -> usize {
    utf16_len(&line[..line.len() - line.trim_start().len()])
}

fn starts_at(cs: &[char], i: usize, s: &str) -> bool {
    (i..).zip(s.chars()).all(|(j, c)| cs.get(j) == Some(&c))
}

fn starts_at_ci(cs: &[char], i: usize, s: &str) -> bool {
    let mut j = i;
    for c in s.chars() {
        match cs.get(j) {
            Some(d) if d.eq_ignore_ascii_case(&c) => j += 1,
            _ => return false,
        }
    }
    true
}

// ---- Brace languages ----

#[derive(Debug, Clone, Copy)]
struct Brace {
    pos: Pos,
    open: bool,
}

/// The `{` and `}` of `cs[start..end]` outside strings and comments (and outside PHP's
/// inline HTML), with their positions; `pos` is the position of `start`.
fn scan_braces(cs: &[char], start: usize, end: usize, mut pos: Pos, fam: Family) -> Vec<Brace> {
    let mut out = Vec::new();
    let mut i = start;
    let line_comments = fam != Family::Css;
    let hash_comments = fam == Family::Php;
    // PHP files start in HTML until `<?php`.
    let mut in_html = fam == Family::Php;
    macro_rules! step {
        () => {{
            pos.advance(cs[i]);
            i += 1;
        }};
    }
    while i < end {
        let c = cs[i];
        let next = cs.get(i + 1).copied();
        if in_html {
            if starts_at(cs, i, "<?php") || starts_at(cs, i, "<?=") {
                in_html = false;
            }
            step!();
            continue;
        }
        match c {
            '?' if fam == Family::Php && next == Some('>') => {
                in_html = true;
                step!();
            }
            '/' if next == Some('*') => {
                step!();
                step!();
                while i < end && !(cs[i] == '*' && cs.get(i + 1) == Some(&'/')) {
                    step!();
                }
                if i < end {
                    step!();
                    step!();
                }
            }
            '/' if next == Some('/') && line_comments => {
                while i < end && cs[i] != '\n' {
                    // `?>` ends a PHP line comment.
                    if fam == Family::Php && cs[i] == '?' && cs.get(i + 1) == Some(&'>') {
                        break;
                    }
                    step!();
                }
            }
            '#' if hash_comments && next != Some('[') => {
                while i < end && cs[i] != '\n' {
                    step!();
                }
            }
            '<' if fam == Family::Php && starts_at(cs, i, "<<<") => {
                // Heredoc / nowdoc: skip to the line that starts with the identifier.
                for _ in 0..3 {
                    step!();
                }
                let mut ident = String::new();
                while i < end && cs[i] != '\n' {
                    if cs[i].is_alphanumeric() || cs[i] == '_' {
                        ident.push(cs[i]);
                    }
                    step!();
                }
                if ident.is_empty() {
                    continue;
                }
                'heredoc: while i < end {
                    // At a line start: skip blanks, compare.
                    step!(); // the '\n'
                    let mut j = i;
                    while j < end && (cs[j] == ' ' || cs[j] == '\t') {
                        j += 1;
                    }
                    if starts_at(cs, j, &ident) {
                        while i < j + ident.chars().count() {
                            step!();
                        }
                        break 'heredoc;
                    }
                    while i < end && cs[i] != '\n' {
                        step!();
                    }
                }
            }
            '"' | '\'' | '`' => {
                let quote = c;
                step!();
                let mut depth = 0usize;
                while i < end {
                    let d = cs[i];
                    if d == '\\' {
                        step!();
                        if i < end {
                            step!();
                        }
                        continue;
                    }
                    if quote == '`' && d == '$' && cs.get(i + 1) == Some(&'{') {
                        depth += 1;
                        step!();
                        step!();
                        continue;
                    }
                    if depth > 0 && d == '}' {
                        depth -= 1;
                        step!();
                        continue;
                    }
                    if depth == 0 && d == quote {
                        step!();
                        break;
                    }
                    // Only PHP and template literals span lines.
                    if d == '\n' && quote != '`' && fam != Family::Php {
                        break;
                    }
                    step!();
                }
            }
            '{' => {
                out.push(Brace { pos, open: true });
                step!();
            }
            '}' => {
                out.push(Brace { pos, open: false });
                step!();
            }
            _ => step!(),
        }
    }
    out
}

const CONTROL: &[&str] = &[
    "if",
    "else",
    "elseif",
    "for",
    "foreach",
    "while",
    "do",
    "switch",
    "try",
    "catch",
    "finally",
    "match",
    "with",
    "loop",
    "unless",
    "select",
    "synchronized",
    "using",
    "lock",
    "unsafe",
    "defer",
];

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Identifier words of a header with their byte offsets.
fn idents(s: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in s.char_indices() {
        if is_ident_char(c) {
            start.get_or_insert(i);
        } else if let Some(st) = start.take() {
            out.push((st, &s[st..i]));
        }
    }
    if let Some(st) = start {
        out.push((st, &s[st..]));
    }
    out
}

/// The identifier right before `(` at byte `paren` (`describe` in `describe('x', () =>`).
fn callee_before(s: &str, paren: usize) -> Option<&str> {
    let head = s[..paren].trim_end();
    let start = head
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_ident_char(*c) || *c == '.')
        .last()
        .map(|(i, _)| i)?;
    let name = &head[start..];
    (!name.is_empty() && !CONTROL.contains(&name)).then_some(name)
}

/// The byte index of the `(` that is still open at the end of `s`.
fn unclosed_paren(s: &str) -> Option<usize> {
    let mut stack = Vec::new();
    for (i, c) in s.char_indices() {
        match c {
            '(' => stack.push(i),
            ')' => {
                stack.pop();
            }
            _ => {}
        }
    }
    stack.pop()
}

/// Kind and crumb text of a brace scope from its header (the text before `{`).
fn classify_code(
    header: &str,
    fam: Family,
    parent: Option<ScopeKind>,
) -> Option<(ScopeKind, String)> {
    let h = header.trim().trim_start_matches('}').trim();
    let words = idents(h);
    let first = words.first().map(|(_, w)| *w).unwrap_or("");
    if CONTROL.contains(&first) {
        let name = match (first, words.get(1).map(|(_, w)| *w)) {
            ("else", Some("if")) => "else if".to_string(),
            _ => first.to_string(),
        };
        return Some((ScopeKind::Control, name));
    }
    let in_type = matches!(
        parent,
        Some(ScopeKind::Class | ScopeKind::Interface | ScopeKind::Trait | ScopeKind::Enum)
    );
    let member_kind = || {
        if in_type || parent == Some(ScopeKind::Object) {
            ScopeKind::Method
        } else {
            ScopeKind::Function
        }
    };
    // Type declarations: `class User extends Model`, `enum Status: string`.
    for (n, (at, w)) in words.iter().enumerate() {
        let kind = match *w {
            "class" | "struct" | "object" | "record" => ScopeKind::Class,
            "interface" | "protocol" => ScopeKind::Interface,
            "trait" | "impl" => ScopeKind::Trait,
            "enum" => ScopeKind::Enum,
            "namespace" | "module" | "package" => ScopeKind::Namespace,
            _ => continue,
        };
        // `Foo::class` and `$this->class` are not declarations.
        let before = h[..*at].trim_end();
        if before.ends_with("::") || before.ends_with("->") || before.ends_with('.') {
            continue;
        }
        if kind == ScopeKind::Namespace {
            let rest = h[at + w.len()..].trim();
            let name: String = rest
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '{')
                .collect();
            return Some((kind, name));
        }
        let name = words
            .get(n + 1)
            .map(|(_, w)| *w)
            .filter(|w| !matches!(*w, "extends" | "implements" | "for" | "where"));
        return Some(match name {
            Some(name) => (kind, name.to_string()),
            None => (ScopeKind::Class, "anonymous class".to_string()),
        });
    }
    // Functions: `function orders(): HasMany`, `fn run()`, `func main()`, `def x`.
    for (at, w) in &words {
        if !matches!(*w, "function" | "fn" | "func" | "fun" | "def") {
            continue;
        }
        let after = h[at + w.len()..]
            .trim_start()
            .trim_start_matches(['*', '&'])
            .trim_start();
        if !after.starts_with('(') {
            if let Some((_, name)) = idents(after).first() {
                return Some((member_kind(), format!("{name}()")));
            }
        }
        // An anonymous function: named after what it is assigned to or passed to.
        return Some(anonymous(&h[..*at], fam));
    }
    if fam == Family::Php {
        return None;
    }
    // Arrow functions: `const total = (items) => {`, `items.forEach((i) => {`.
    if let Some(arrow) = h.find("=>") {
        let head = &h[..arrow];
        if let Some(name) = assigned_name(head) {
            return Some((member_kind(), format!("{name}()")));
        }
        return Some(anonymous(head, fam));
    }
    // Methods and C-like functions: `async save(order)`, `get total(): number`,
    // `public void run()`.
    if let Some(paren) = h.find('(') {
        let head = &h[..paren];
        let head_words = idents(head);
        if !head.contains('=')
            && unclosed_paren(h).is_none()
            && !head.trim_end().ends_with('.')
            && !head.contains(':')
            && !head_words.is_empty()
            && fam != Family::Css
        {
            let name = head_words.last().map(|(_, w)| *w).unwrap_or("");
            if !CONTROL.contains(&name) && !name.is_empty() {
                return Some((member_kind(), format!("{name}()")));
            }
        }
    }
    // Named object literals: `const config =`, `methods:`, `export default`.
    if fam == Family::Script {
        let t = h.trim_end();
        if let Some(head) = t.strip_suffix('=') {
            if let Some(name) = idents(head).last().map(|(_, w)| *w) {
                return Some((ScopeKind::Object, name.to_string()));
            }
        }
        if let Some(head) = t.strip_suffix(':') {
            if let Some(name) = idents(head).last().map(|(_, w)| *w) {
                return Some((ScopeKind::Object, name.to_string()));
            }
        }
        if t == "export default" {
            return Some((ScopeKind::Object, "export default".to_string()));
        }
    }
    None
}

/// `total` in `const total = (`, `this.total = `, `total: ` and `total(`; `None` when the
/// function is not assigned.
fn assigned_name(head: &str) -> Option<String> {
    let t = head.trim();
    // Strip the parameter list.
    let before_params = match unclosed_or_params(t) {
        Some(i) => &t[..i],
        None => t,
    };
    let b = before_params.trim_end();
    let b = b.strip_suffix("async").unwrap_or(b).trim_end();
    let name_part = if let Some(x) = b.strip_suffix('=') {
        x
    } else {
        b.strip_suffix(':')?
    };
    let name = idents(name_part).last().map(|(_, w)| w.to_string())?;
    // `foo(a, b = (x) => {`: an argument default, not an assignment.
    if unclosed_paren(name_part).is_some() {
        return None;
    }
    Some(name)
}

/// The start of the parameter list of an arrow function head (`(a, b)` or a bare `a`).
fn unclosed_or_params(t: &str) -> Option<usize> {
    let t_trim = t.trim_end();
    if t_trim.ends_with(')') {
        let mut depth = 0i32;
        for (i, c) in t_trim.char_indices().rev() {
            match c {
                ')' => depth += 1,
                '(' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        None
    } else {
        // A single bare parameter: `x =>`.
        let end = t_trim.len();
        let start = t_trim
            .char_indices()
            .rev()
            .take_while(|(_, c)| is_ident_char(*c))
            .last()
            .map(|(i, _)| i)
            .unwrap_or(end);
        Some(start)
    }
}

/// An anonymous function (`function () {`, `() => {`): named after the variable it is
/// assigned to, else after the call it is passed to (`forEach callback`).
fn anonymous(head: &str, fam: Family) -> (ScopeKind, String) {
    let t = head.trim_end();
    let t = t.strip_suffix("async").unwrap_or(t).trim_end();
    let t = t.strip_suffix("static").unwrap_or(t).trim_end();
    if let Some(x) = t.strip_suffix('=').or_else(|| t.strip_suffix(':')) {
        if unclosed_paren(x).is_none() {
            if let Some(name) = idents(x).last().map(|(_, w)| *w) {
                return (
                    ScopeKind::Function,
                    format!("{}()", name.trim_start_matches('$')),
                );
            }
        }
    }
    if let Some(paren) = unclosed_paren(t) {
        if let Some(callee) = callee_before(t, paren) {
            let short = callee.rsplit(['.']).next().unwrap_or(callee);
            let short = short.rsplit("->").next().unwrap_or(short);
            return (ScopeKind::Closure, format!("{short} callback"));
        }
    }
    let name = if fam == Family::Php {
        "function()"
    } else {
        "callback"
    };
    (ScopeKind::Closure, name.to_string())
}

/// The text of a CSS rule header with whitespace collapsed.
fn css_name(header: &str) -> String {
    header
        .trim()
        .trim_start_matches('}')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The header of the block opened by the `{` at `open`: its first line and its text.
fn header_of(lines: &[&str], open: Pos, fam: Family) -> (usize, String) {
    let Some(open_line) = lines.get(open.line) else {
        return (open.line, String::new());
    };
    let before = &open_line[..byte_col(open_line, open.col as u32)];
    let mut start = header_start(lines, open.line);
    // CSS selector lists: `.card,` on the lines above.
    if fam == Family::Css {
        while start > 0 && lines[start - 1].trim_end().ends_with(',') {
            start -= 1;
        }
    }
    let start = start.min(open.line);
    let mut text = String::new();
    for line in &lines[start..open.line] {
        text.push_str(line.trim());
        text.push(' ');
    }
    text.push_str(before.trim());
    (start, text)
}

/// The last statement of a header (`if ($b)` in `$a = 1; if ($b)`, the code after PHP's
/// `<?php`).
fn last_statement(header: &str, fam: Family) -> String {
    let mut h = header;
    if fam == Family::Php {
        for open in ["<?php", "<?="] {
            if let Some(i) = h.rfind(open) {
                h = &h[i + open.len()..];
            }
        }
    }
    let mut depth = 0i32;
    let mut cut = 0;
    for (i, c) in h.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ';' | '{' | '}' if depth <= 0 => cut = i + 1,
            _ => {}
        }
    }
    // `} else` keeps its `}` out; a leading `}` is stripped by the classifier anyway.
    h[cut..].trim().to_string()
}

/// The scopes of a brace language from its braces.
fn brace_scopes(
    braces: &[Brace],
    lines: &[&str],
    fam: Family,
    outer: Option<usize>,
    out: &mut Vec<Scope>,
) {
    // Open braces: the index of their scope, or None for plain blocks.
    let mut stack: Vec<Option<usize>> = Vec::new();
    for b in braces {
        if b.open {
            let parent = stack.iter().rev().find_map(|s| *s).or(outer);
            let parent_kind = parent.map(|p| out[p].kind);
            let (start, header) = header_of(lines, b.pos, fam);
            let header = last_statement(&header, fam);
            let classified = if fam == Family::Css {
                let name = css_name(&header);
                (!name.is_empty()).then_some((ScopeKind::Rule, name))
            } else {
                classify_code(&header, fam, parent_kind)
            };
            match classified {
                Some((kind, name)) => {
                    let text = lines.get(start).copied().unwrap_or("");
                    let start_column = indent_col(text);
                    let base = name.trim_end_matches("()").trim_end_matches(" callback");
                    let column = text
                        .find(base)
                        .filter(|_| !base.is_empty())
                        .map(|i| utf16_len(&text[..i]))
                        .unwrap_or(start_column);
                    out.push(Scope {
                        kind,
                        name,
                        line: start as u32,
                        column: column as u32,
                        start_column: start_column as u32,
                        open_line: b.pos.line as u32,
                        end_line: lines.len().saturating_sub(1) as u32,
                        end_column: lines.last().map(|l| utf16_len(l)).unwrap_or(0) as u32,
                        parent: parent.map(|p| p as u32),
                        sticky: true,
                        crumb: kind.is_crumb(),
                    });
                    stack.push(Some(out.len() - 1));
                }
                None => stack.push(None),
            }
        } else if let Some(Some(i)) = stack.pop() {
            out[i].end_line = b.pos.line as u32;
            out[i].end_column = (b.pos.col + 1) as u32;
        }
    }
}

/// Drop scopes that start and end on one line (nothing to stick) and re-link the parents.
fn compact(scopes: Vec<Scope>) -> Vec<Scope> {
    let keep: Vec<bool> = scopes.iter().map(|s| s.end_line > s.line).collect();
    let mut new_index = vec![None; scopes.len()];
    let mut out: Vec<Scope> = Vec::new();
    for (i, s) in scopes.iter().enumerate() {
        if !keep[i] {
            continue;
        }
        let mut parent = s.parent.map(|p| p as usize);
        while let Some(p) = parent {
            if keep[p] {
                break;
            }
            parent = scopes[p].parent.map(|q| q as usize);
        }
        let mut scope = s.clone();
        scope.parent = parent.and_then(|p| new_index[p]).map(|p: usize| p as u32);
        new_index[i] = Some(out.len());
        out.push(scope);
    }
    // Start order (script and style scopes are appended after their tag's siblings).
    let mut order: Vec<usize> = (0..out.len()).collect();
    order.sort_by_key(|&i| (out[i].line, out[i].start_column, i));
    if order.iter().enumerate().all(|(a, b)| a == *b) {
        return out;
    }
    let mut position = vec![0usize; out.len()];
    for (new, &old) in order.iter().enumerate() {
        position[old] = new;
    }
    order
        .iter()
        .map(|&old| {
            let mut s = out[old].clone();
            s.parent = s.parent.map(|p| position[p as usize] as u32);
            s
        })
        .collect()
}

// ---- Markup ----

/// Elements without an end tag.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr", "!doctype",
];

/// `div#main.card.wide` from a tag name and its attributes.
fn tag_label(name: &str, attrs: &str) -> String {
    let mut label = name.to_string();
    if let Some(id) = attr_value(attrs, "id") {
        if !id.is_empty() {
            label.push('#');
            label.push_str(id.trim());
        }
    }
    if let Some(class) = attr_value(attrs, "class") {
        for c in class.split_whitespace().take(3) {
            // Template expressions in class lists (`{{ $x }}`) are not class names.
            if c.contains(['{', '}', '<', '>', '$', '(']) {
                break;
            }
            label.push('.');
            label.push_str(c);
        }
    }
    label
}

/// The value of an attribute in an opening tag's attribute text.
fn attr_value<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let mut search = 0;
    while let Some(i) = attrs[search..].find(name) {
        let at = search + i;
        search = at + name.len();
        let before_ok = at == 0 || attrs[..at].ends_with(char::is_whitespace);
        let rest = attrs[at + name.len()..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let rest = rest[1..].trim_start();
        let quote = rest.chars().next()?;
        if quote == '"' || quote == '\'' {
            let inner = &rest[1..];
            return inner.find(quote).map(|e| &inner[..e]);
        }
        return rest.split_whitespace().next();
    }
    None
}

fn markup_scopes(cs: &[char], lines: &[&str], language: &str, out: &mut Vec<Scope>) {
    let html = language != "xml";
    let mut pos = Pos::default();
    let mut i = 0;
    // Open elements: (lower-case name, scope index).
    let mut stack: Vec<(String, usize)> = Vec::new();
    let n = cs.len();
    macro_rules! step {
        () => {{
            pos.advance(cs[i]);
            i += 1;
        }};
    }
    macro_rules! skip_to {
        ($end:expr) => {{
            while i < n && !starts_at(cs, i, $end) {
                step!();
            }
            for _ in 0..$end.chars().count() {
                if i < n {
                    step!();
                }
            }
        }};
    }
    let templates = language != "xml" && language != "html" && language != "svg";
    while i < n {
        if starts_at(cs, i, "<!--") {
            skip_to!("-->");
            continue;
        }
        if starts_at(cs, i, "<?") {
            skip_to!("?>");
            continue;
        }
        if starts_at(cs, i, "<![CDATA[") {
            skip_to!("]]>");
            continue;
        }
        if templates && starts_at(cs, i, "{{--") {
            skip_to!("--}}");
            continue;
        }
        if templates && (starts_at(cs, i, "{{") || starts_at(cs, i, "{%") || starts_at(cs, i, "{#"))
        {
            let close = match cs[i + 1] {
                '{' => "}}",
                '%' => "%}",
                _ => "#}",
            };
            skip_to!(close);
            continue;
        }
        if cs[i] != '<' {
            step!();
            continue;
        }
        let start = pos;
        let closing = cs.get(i + 1) == Some(&'/');
        let name_start = i + if closing { 2 } else { 1 };
        if !cs
            .get(name_start)
            .is_some_and(|c| c.is_alphabetic() || *c == '!')
        {
            step!();
            continue;
        }
        let mut j = name_start;
        while j < n && (cs[j].is_alphanumeric() || matches!(cs[j], '-' | ':' | '.' | '_' | '!')) {
            j += 1;
        }
        let raw_name: String = cs[name_start..j].iter().collect();
        let key = raw_name.to_ascii_lowercase();
        // Attributes up to `>`, honoring quotes.
        let mut attrs = String::new();
        let mut quote: Option<char> = None;
        while i < j {
            step!();
        }
        while i < n {
            let c = cs[i];
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => {}
                None if c == '"' || c == '\'' => quote = Some(c),
                None if c == '>' => break,
                None => {}
            }
            attrs.push(c);
            step!();
        }
        let self_closing = attrs.trim_end().ends_with('/');
        let end_of_tag = pos;
        if i < n {
            step!(); // '>'
        }
        if closing {
            if let Some(k) = stack.iter().rposition(|(name, _)| *name == key) {
                while stack.len() > k {
                    let (_, idx) = stack.pop().unwrap();
                    out[idx].end_line = pos.line as u32;
                    out[idx].end_column = pos.col as u32;
                }
            }
            continue;
        }
        if self_closing || (html && VOID.contains(&key.as_str())) || key.starts_with('!') {
            continue;
        }
        let parent = stack.last().map(|(_, idx)| *idx);
        out.push(Scope {
            kind: ScopeKind::Tag,
            name: tag_label(if html { &key } else { &raw_name }, &attrs),
            line: start.line as u32,
            column: (start.col + 1) as u32,
            start_column: start.col as u32,
            open_line: end_of_tag.line as u32,
            end_line: lines.len().saturating_sub(1) as u32,
            end_column: lines.last().map(|l| utf16_len(l)).unwrap_or(0) as u32,
            parent: parent.map(|p| p as u32),
            sticky: true,
            crumb: true,
        });
        let idx = out.len() - 1;
        // Script and style bodies are code.
        if html && (key == "script" || key == "style") {
            let close = if key == "script" {
                "</script"
            } else {
                "</style"
            };
            let body_start = i;
            let body_pos = pos;
            let mut k = i;
            while k < n && !starts_at_ci(cs, k, close) {
                k += 1;
            }
            let fam = if key == "style" {
                Family::Css
            } else {
                Family::Script
            };
            let braces = scan_braces(cs, body_start, k, body_pos, fam);
            brace_scopes(&braces, lines, fam, Some(idx), out);
            while i < k {
                step!();
            }
        }
        stack.push((key, idx));
    }
}

// ---- Lua ----

fn lua_scopes(cs: &[char], lines: &[&str], out: &mut Vec<Scope>) {
    let n = cs.len();
    let mut pos = Pos::default();
    let mut i = 0;
    let mut stack: Vec<usize> = Vec::new();
    // `for` / `while` wait for their `do`.
    let mut pending_do = 0usize;
    macro_rules! step {
        () => {{
            pos.advance(cs[i]);
            i += 1;
        }};
    }
    // `[[`, `[=[`: the closing bracket, or None.
    let long_bracket = |at: usize| -> Option<String> {
        if cs.get(at) != Some(&'[') {
            return None;
        }
        let mut j = at + 1;
        let mut eq = 0;
        while cs.get(j) == Some(&'=') {
            eq += 1;
            j += 1;
        }
        (cs.get(j) == Some(&'[')).then(|| format!("]{}]", "=".repeat(eq)))
    };
    while i < n {
        let c = cs[i];
        if c == '-' && cs.get(i + 1) == Some(&'-') {
            step!();
            step!();
            if let Some(close) = long_bracket(i) {
                while i < n && !starts_at(cs, i, &close) {
                    step!();
                }
                for _ in 0..close.len() {
                    if i < n {
                        step!();
                    }
                }
            } else {
                while i < n && cs[i] != '\n' {
                    step!();
                }
            }
            continue;
        }
        if c == '[' {
            if let Some(close) = long_bracket(i) {
                while i < n && !starts_at(cs, i, &close) {
                    step!();
                }
                for _ in 0..close.len() {
                    if i < n {
                        step!();
                    }
                }
                continue;
            }
        }
        if c == '"' || c == '\'' {
            step!();
            while i < n && cs[i] != c && cs[i] != '\n' {
                if cs[i] == '\\' {
                    step!();
                }
                if i < n {
                    step!();
                }
            }
            if i < n {
                step!();
            }
            continue;
        }
        if !(c.is_alphabetic() || c == '_')
            || (i > 0 && (is_ident_char(cs[i - 1]) || cs[i - 1] == '.' || cs[i - 1] == ':'))
        {
            step!();
            continue;
        }
        let word_pos = pos;
        let mut word = String::new();
        while i < n && (cs[i].is_alphanumeric() || cs[i] == '_') {
            word.push(cs[i]);
            step!();
        }
        let text = lines.get(word_pos.line).copied().unwrap_or("");
        let at = byte_col(text, word_pos.col as u32);
        let opened: Option<(ScopeKind, String, usize)> = match word.as_str() {
            "function" => {
                let after = &text[(at + "function".len()).min(text.len())..];
                let name = after.split('(').next().unwrap_or("").trim();
                if !name.is_empty() {
                    let col = text[at..]
                        .find(name)
                        .map(|k| utf16_len(&text[..at + k]))
                        .unwrap_or(word_pos.col);
                    Some((ScopeKind::Function, format!("{name}()"), col))
                } else {
                    let (kind, name) = anonymous(&text[..at], Family::Lua);
                    let base = name.trim_end_matches("()").trim_end_matches(" callback");
                    let col = text
                        .find(base)
                        .map(|k| utf16_len(&text[..k]))
                        .unwrap_or(word_pos.col);
                    let name = if name == "callback" {
                        "function()".to_string()
                    } else {
                        name
                    };
                    Some((kind, name, col))
                }
            }
            "if" | "repeat" => Some((ScopeKind::Control, word.clone(), word_pos.col)),
            "for" | "while" => {
                pending_do += 1;
                Some((ScopeKind::Control, word.clone(), word_pos.col))
            }
            "do" if pending_do > 0 => {
                pending_do -= 1;
                None
            }
            "do" => Some((ScopeKind::Control, word.clone(), word_pos.col)),
            "end" | "until" => {
                if let Some(idx) = stack.pop() {
                    out[idx].end_line = pos.line as u32;
                    out[idx].end_column = pos.col as u32;
                }
                None
            }
            _ => None,
        };
        if let Some((kind, name, column)) = opened {
            out.push(Scope {
                kind,
                name,
                line: word_pos.line as u32,
                column: column as u32,
                start_column: indent_col(text) as u32,
                open_line: word_pos.line as u32,
                end_line: lines.len().saturating_sub(1) as u32,
                end_column: lines.last().map(|l| utf16_len(l)).unwrap_or(0) as u32,
                parent: stack.last().map(|p| *p as u32),
                sticky: true,
                crumb: kind.is_crumb(),
            });
            stack.push(out.len() - 1);
        }
    }
}

// ---- Custom folding regions ----

#[derive(Debug, PartialEq, Eq)]
enum Marker {
    Start { name: String, collapsed: bool },
    End,
}

/// A region marker comment on a line: `// region Name`, `//endregion`, `#region`,
/// `<!-- region -->`, `/* region */`, `-- region`, `{{-- region --}}`, `{# region #}`
/// and `// <editor-fold desc="Name" defaultstate="collapsed">` / `// </editor-fold>`.
fn region_marker(line: &str, prefixes: &[&str]) -> Option<Marker> {
    let t = line.trim();
    let body = prefixes.iter().find_map(|p| t.strip_prefix(p))?;
    let body = body.trim_start_matches(['/', '-', '*']).trim();
    let body = ["-->", "--}}", "#}", "*/"]
        .iter()
        .find_map(|s| body.strip_suffix(s))
        .unwrap_or(body)
        .trim();
    let keyword = |kw: &str| {
        body.strip_prefix(kw)
            .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
            .map(str::trim)
    };
    if keyword("endregion").is_some() {
        return Some(Marker::End);
    }
    if let Some(name) = keyword("region") {
        return Some(Marker::Start {
            name: name.to_string(),
            collapsed: false,
        });
    }
    if body.starts_with("</editor-fold") {
        return Some(Marker::End);
    }
    if let Some(attrs) = body.strip_prefix("<editor-fold") {
        let attrs = attrs.trim_end_matches('>').trim_end_matches('/');
        return Some(Marker::Start {
            name: attr_value(attrs, "desc").unwrap_or("").to_string(),
            collapsed: attr_value(attrs, "defaultstate").is_some_and(|v| v == "collapsed"),
        });
    }
    None
}

/// The comment starts a region marker may use in a language. `#` is a selector in CSS and
/// a private name in JavaScript, `--` a decrement.
fn comment_prefixes(language: &str) -> &'static [&'static str] {
    match family(language) {
        Family::Css => &["/*", "//"],
        Family::Script => &["//", "/*"],
        Family::Php => &["//", "#", "/*"],
        Family::Lua => &["--"],
        Family::Markup => &["<!--", "{{--", "{#", "//", "/*"],
        Family::Braces => &["//", "#", "/*"],
        Family::None => &["<!--", "{{--", "{#", "/*", "//", "--", "#", ";"],
    }
}

/// The custom folding regions of a document, outermost first by start line.
fn regions(lines: &[&str], language: &str) -> Vec<Region> {
    let prefixes = comment_prefixes(language);
    let mut out = Vec::new();
    let mut open: Vec<(usize, String, bool)> = Vec::new();
    for (n, line) in lines.iter().enumerate() {
        match region_marker(line, prefixes) {
            Some(Marker::Start { name, collapsed }) => open.push((n, name, collapsed)),
            Some(Marker::End) => {
                if let Some((start, name, collapsed)) = open.pop() {
                    out.push(Region {
                        name,
                        line: start as u32,
                        column: indent_col(lines[start]) as u32,
                        end_line: n as u32,
                        end_column: utf16_len(line) as u32,
                        collapsed,
                    });
                }
            }
            None => {}
        }
    }
    out.sort_by_key(|r| (r.line, std::cmp::Reverse(r.end_line)));
    out
}

// ---- Structure view ----

fn node_line(node: &Value) -> Option<u32> {
    node.get("line").and_then(Value::as_u64).map(|l| l as u32)
}

/// Group the nodes of a Structure view tree (JSON objects with a 0-based `line` and
/// optional `children`) under the custom folding regions around them, like PHPStorm. A
/// region node is `{ name, kind: "region", line, column, children }`; regions inside a
/// class group its members.
pub fn group_by_regions(nodes: Vec<Value>, regions: &[Region]) -> Vec<Value> {
    group_level(nodes, regions, None)
}

fn group_level(nodes: Vec<Value>, regions: &[Region], after: Option<u32>) -> Vec<Value> {
    // Members first.
    let nodes: Vec<Value> = nodes
        .into_iter()
        .map(|mut n| {
            let line = node_line(&n);
            if let Some(children) = n.get_mut("children").and_then(Value::as_array_mut) {
                let taken = std::mem::take(children);
                *children = group_level(taken, regions, line);
            }
            n
        })
        .collect();
    let inside = |r: &Region, n: &Value| node_line(n).is_some_and(|l| l > r.line && l < r.end_line);
    // Regions of this level: they start in the parent's body and hold one of its nodes.
    let applicable: Vec<&Region> = regions
        .iter()
        .filter(|r| after.is_none_or(|a| r.line > a))
        .filter(|r| nodes.iter().any(|n| inside(r, n)))
        .collect();
    nest(nodes, &applicable)
}

fn nest(nodes: Vec<Value>, regions: &[&Region]) -> Vec<Value> {
    // The outermost regions of this list.
    let outer: Vec<&Region> = regions
        .iter()
        .filter(|r| {
            !regions.iter().any(|o| {
                !std::ptr::eq(*o, **r)
                    && o.line <= r.line
                    && o.end_line >= r.end_line
                    && (o.line, o.end_line) != (r.line, r.end_line)
            })
        })
        .copied()
        .collect();
    let mut rest: Vec<Value> = Vec::new();
    let mut groups: Vec<(&Region, Vec<Value>)> = outer.iter().map(|r| (*r, Vec::new())).collect();
    for n in nodes {
        let l = node_line(&n);
        match groups
            .iter_mut()
            .find(|(r, _)| l.is_some_and(|l| l > r.line && l < r.end_line))
        {
            Some((_, list)) => list.push(n),
            None => rest.push(n),
        }
    }
    for (r, members) in groups {
        let inner: Vec<&Region> = regions
            .iter()
            .filter(|o| !std::ptr::eq(**o, r) && o.line > r.line && o.end_line < r.end_line)
            .copied()
            .collect();
        let children = nest(members, &inner);
        rest.push(serde_json::json!({
            "name": if r.name.is_empty() { "region" } else { r.name.as_str() },
            "kind": "region",
            "detail": "",
            "modifiers": {},
            "offset": 0,
            "line": r.line,
            "column": r.column,
            "children": children,
        }));
    }
    rest.sort_by_key(|n| node_line(n).unwrap_or(0));
    rest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(s: &EditorStructure) -> Vec<String> {
        s.scopes.iter().map(|s| s.name.clone()).collect()
    }

    fn chain(s: &EditorStructure, line: u32) -> Vec<String> {
        s.scopes
            .iter()
            .filter(|sc| sc.line <= line && sc.end_line >= line)
            .map(|sc| sc.name.clone())
            .collect()
    }

    const USER: &str = r#"<?php

namespace App\Models;

class User extends Model
{
    // region Relations
    public function orders(): HasMany
    {
        if ($this->active) {
            return $this->hasMany(Order::class);
        }
        return $this->hasMany(Order::class)->where(fn ($o) => $o->paid);
    }
    // endregion

    public function label(
        string $prefix,
    ): string {
        $s = '{ not a block';
        return array_map(function ($x) {
            return $x;
        }, []);
    }
}
"#;

    #[test]
    fn php_scopes_nest_and_name_their_crumbs() {
        let s = editor_structure(USER, "php");
        assert_eq!(
            names(&s),
            ["User", "orders()", "if", "label()", "array_map callback"]
        );
        let user = &s.scopes[0];
        assert_eq!(
            (user.kind, user.line, user.open_line, user.end_line),
            (ScopeKind::Class, 4, 5, 24)
        );
        assert_eq!(user.column, 6);
        let orders = &s.scopes[1];
        assert_eq!(
            (orders.kind, orders.line, orders.end_line, orders.parent),
            (ScopeKind::Method, 7, 13, Some(0))
        );
        assert_eq!(orders.column, 20);
        let iff = &s.scopes[2];
        assert_eq!(
            (iff.kind, iff.crumb, iff.sticky, iff.parent),
            (ScopeKind::Control, false, true, Some(1))
        );
        // A multi-line parameter list: the header starts at `function label(`.
        assert_eq!(s.scopes[3].line, 16);
        assert_eq!(chain(&s, 21), ["User", "label()", "array_map callback"]);
    }

    #[test]
    fn script_scopes() {
        let js = "export class Cart {\n  async total(items) {\n    items.forEach((i) => {\n      x(i);\n    });\n    for (const i of items) {\n      y();\n    }\n  }\n  handler = () => {\n    z();\n  };\n}\nconst config = {\n  methods: {\n    save() {\n      a();\n    },\n  },\n};\nfunction run() {\n  const s = `${a} {`;\n  return s;\n}\n";
        let s = editor_structure(js, "javascript");
        assert_eq!(
            names(&s),
            [
                "Cart",
                "total()",
                "forEach callback",
                "for",
                "handler()",
                "config",
                "methods",
                "save()",
                "run()"
            ]
        );
        assert_eq!(s.scopes[1].kind, ScopeKind::Method);
        assert_eq!(s.scopes[4].kind, ScopeKind::Method);
        assert_eq!(s.scopes[7].kind, ScopeKind::Method);
        assert_eq!(s.scopes[8].kind, ScopeKind::Function);
        assert_eq!(s.scopes[8].end_line, 23);
    }

    #[test]
    fn css_rules() {
        let css = "@media (max-width: 600px) {\n  .card,\n  .panel {\n    color: red;\n  }\n}\n/* { */\na { b: c; }\n";
        let s = editor_structure(css, "css");
        assert_eq!(names(&s), ["@media (max-width: 600px)", ".card, .panel"]);
        assert_eq!(s.scopes[1].line, 1);
        assert_eq!(s.scopes[1].parent, Some(0));
    }

    #[test]
    fn markup_elements_with_script_and_style() {
        let html = "<!DOCTYPE html>\n<html>\n<body>\n  <div id=\"app\" class=\"card wide\">\n    <img src=\"x.png\">\n    <p>one\n    two</p>\n  </div>\n  <script>\n    function boot() {\n      go();\n    }\n  </script>\n</body>\n</html>\n";
        let s = editor_structure(html, "html");
        assert_eq!(
            names(&s),
            ["html", "body", "div#app.card.wide", "p", "script", "boot()"]
        );
        let div = &s.scopes[2];
        assert_eq!((div.line, div.end_line, div.start_column), (3, 7, 2));
        assert_eq!(s.scopes[5].parent, Some(4));
        assert_eq!(chain(&s, 10), ["html", "body", "script", "boot()"]);

        let vue = "<template>\n  <ul>\n    <li v-for=\"i in items\">{{ i }}</li>\n  </ul>\n</template>\n<script setup lang=\"ts\">\nconst x = computed(() => {\n  return 1;\n});\n</script>\n";
        let s = editor_structure(vue, "vue");
        assert_eq!(names(&s), ["template", "ul", "script", "computed callback"]);
    }

    #[test]
    fn lua_functions_and_blocks() {
        let lua = "local M = {}\n\nfunction M.run(xs)\n  for _, x in ipairs(xs) do\n    if x then\n      print(\"end\")\n    end\n  end\nend\n\nlocal cb = function(a)\n  -- end\n  return a\nend\n\nlocal t = { f = 1 } if t then return end\n";
        let s = editor_structure(lua, "lua");
        assert_eq!(names(&s), ["M.run()", "for", "if", "cb()"]);
        assert_eq!((s.scopes[0].line, s.scopes[0].end_line), (2, 8));
        assert_eq!(s.scopes[0].column, 9);
        assert_eq!(s.scopes[2].parent, Some(1));
        assert!(!s.scopes[1].crumb);
    }

    #[test]
    fn region_markers() {
        let all = comment_prefixes("text");
        let m = |l: &str| region_marker(l, all);
        let start = |name: &str| {
            Some(Marker::Start {
                name: name.to_string(),
                collapsed: false,
            })
        };
        assert_eq!(m("    // region Relations"), start("Relations"));
        assert_eq!(m("//region"), start(""));
        assert_eq!(m("#region Helpers"), start("Helpers"));
        assert_eq!(m("-- region Lua stuff"), start("Lua stuff"));
        assert_eq!(m("<!-- region Header -->"), start("Header"));
        assert_eq!(m("/* region Colors */"), start("Colors"));
        assert_eq!(m("{{-- region Blade --}}"), start("Blade"));
        assert_eq!(m("// endregion"), Some(Marker::End));
        assert_eq!(m("#endregion"), Some(Marker::End));
        assert_eq!(m("<!-- endregion -->"), Some(Marker::End));
        assert_eq!(
            m("// <editor-fold desc=\"Getters\" defaultstate=\"collapsed\">"),
            Some(Marker::Start {
                name: "Getters".into(),
                collapsed: true
            })
        );
        assert_eq!(m("// </editor-fold>"), Some(Marker::End));
        assert_eq!(m("// regional settings"), None);
        assert_eq!(m("$region = 1;"), None);
        // `#region` is an id selector in CSS.
        assert_eq!(region_marker("#region {", comment_prefixes("css")), None);
    }

    #[test]
    fn regions_nest_and_ignore_unbalanced_markers() {
        let text = "a\n  // region Outer\n  // region Inner\n  x\n  // endregion\n  // endregion\n// endregion\n";
        let r = regions(&lines_of(text), "php");
        assert_eq!(r.len(), 2);
        assert_eq!(
            (r[0].name.as_str(), r[0].line, r[0].end_line, r[0].column),
            ("Outer", 1, 5, 2)
        );
        assert_eq!(
            (r[1].name.as_str(), r[1].line, r[1].end_line),
            ("Inner", 2, 4)
        );
        assert_eq!(r[1].end_column, 14);
        assert!(editor_structure("# region\n# endregion\n", "markdown")
            .regions
            .is_empty());
        assert_eq!(editor_structure(USER, "php").regions[0].name, "Relations");
    }

    #[test]
    fn structure_nodes_are_grouped_by_regions() {
        let s = editor_structure(USER, "php");
        let nodes = vec![serde_json::json!({
            "name": "User", "kind": "class", "line": 4,
            "children": [
                { "name": "orders", "kind": "method", "line": 7 },
                { "name": "label", "kind": "method", "line": 16 },
            ],
        })];
        let grouped = group_by_regions(nodes, &s.regions);
        let members = grouped[0]["children"].as_array().unwrap();
        assert_eq!(members.len(), 2);
        assert_eq!(members[0]["kind"], "region");
        assert_eq!(members[0]["name"], "Relations");
        assert_eq!(members[0]["children"][0]["name"], "orders");
        assert_eq!(members[1]["name"], "label");

        let top = group_by_regions(
            vec![
                serde_json::json!({ "name": "a", "line": 1 }),
                serde_json::json!({ "name": "b", "line": 3 }),
                serde_json::json!({ "name": "c", "line": 5 }),
            ],
            &regions(
                &lines_of("x\na\n// region R\nb\n// endregion\nc\n"),
                "javascript",
            ),
        );
        let names: Vec<&str> = top.iter().map(|n| n["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["a", "R", "c"]);
    }

    #[test]
    fn php_inline_html_and_heredocs_are_skipped() {
        let php = "<div>{ <?php if ($a) { ?>\n<b>{</b>\n<?php } ?>\n<?php\n$x = <<<EOT\n  { not code\n  EOT;\nfunction f()\n{\n  return 1;\n}\n";
        let s = editor_structure(php, "php");
        assert_eq!(names(&s), ["if", "f()"]);
        assert_eq!(s.scopes[1].line, 7);
    }
}
