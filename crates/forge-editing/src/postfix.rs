//! Postfix completion: templates that wrap the expression before the caret, typed as
//! `expr.key` (`$user.if`, `items.forof`, `list.ipairs`), like PHPStorm's postfix templates.
//!
//! [`complete_postfix`] finds the expression in the line before the caret and expands every
//! applicable template; the UI offers them in code completion and applies one on `Tab`.
//! [`templates`] lists all templates with an example for Settings | Editor | General |
//! Postfix Completion.
//!
//! Template bodies use these markers: `«expr»` (the expression), `«not»` (the negated
//! expression), `«end»` (the caret), `«name»` (a variable name suggested from the expression,
//! selected) and `«item»` (an element name for loops, selected). `\n` starts a new line at
//! the statement's indentation, `\t` adds one indent unit.

use serde::{Deserialize, Serialize};

use crate::lang::Lang;
use crate::text::{classify, indent_of, u16_len, u16_len_chars, Class};

/// Where a template applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// The expression starts a statement (`if`, `return`, `foreach`); takes the whole
    /// statement before the caret, so `$a > 0.if` checks `$a > 0`.
    Statement,
    /// Anywhere; takes the closest expression (`$user->name.isset`).
    Expression,
    /// A class name (`Foo.new`).
    ClassName,
}

struct Def {
    lang: Lang,
    key: &'static str,
    scope: Scope,
    body: &'static str,
    description: &'static str,
}

const fn t(
    lang: Lang,
    key: &'static str,
    scope: Scope,
    body: &'static str,
    description: &'static str,
) -> Def {
    Def {
        lang,
        key,
        scope,
        body,
        description,
    }
}

use Lang::{Js, Lua, Php};
use Scope::{ClassName as Cls, Expression as Ex, Statement as St};

const IF_FALSE: &str = "Checks boolean expression to be 'false'";
const IF_TRUE: &str = "Checks boolean expression to be 'true'";
const NOT: &str = "Negates boolean expression";
const NOT_NULL: &str = "Checks expression to be not-null";
const NULL: &str = "Checks expression to be null";
const PAR: &str = "Parenthesizes current expression";
const RETURN: &str = "Returns value from containing function";
const WHILE: &str = "Loops while the expression is 'true'";
const SWITCH: &str = "Produces switch over the expression";
const THROW: &str = "Throws the expression";
const NEW: &str = "Creates an instance of the class";

static TEMPLATES: &[Def] = &[
    // PHP
    t(
        Php,
        "echo",
        St,
        "echo «expr»;«end»",
        "Outputs the expression with echo",
    ),
    t(Php, "else", St, "if («not») {\n\t«end»\n}", IF_FALSE),
    t(
        Php,
        "empty",
        Ex,
        "empty(«expr»)«end»",
        "Checks whether the expression is empty",
    ),
    t(
        Php,
        "for",
        St,
        "for ($i = 0; $i < «expr»; $i++) {\n\t«end»\n}",
        "Iterates with an index up to the expression",
    ),
    t(
        Php,
        "forek",
        St,
        "foreach («expr» as $key => «item») {\n\t«end»\n}",
        "Iterates over keys and values",
    ),
    t(
        Php,
        "foreach",
        St,
        "foreach («expr» as «item») {\n\t«end»\n}",
        "Iterates over the values",
    ),
    t(
        Php,
        "forr",
        St,
        "for ($i = «expr»; $i >= 0; $i--) {\n\t«end»\n}",
        "Iterates with an index down from the expression",
    ),
    t(Php, "if", St, "if («expr») {\n\t«end»\n}", IF_TRUE),
    t(
        Php,
        "isset",
        Ex,
        "isset(«expr»)«end»",
        "Checks whether the variable is set",
    ),
    t(Php, "new", Cls, "new «expr»(«end»)", NEW),
    t(Php, "not", Ex, "«not»«end»", NOT),
    t(
        Php,
        "notnull",
        St,
        "if («expr» !== null) {\n\t«end»\n}",
        NOT_NULL,
    ),
    t(Php, "null", St, "if («expr» === null) {\n\t«end»\n}", NULL),
    t(Php, "par", Ex, "(«expr»)«end»", PAR),
    t(Php, "return", St, "return «expr»;«end»", RETURN),
    t(
        Php,
        "sprintf",
        Ex,
        "sprintf(«expr», «end»)",
        "Wraps the expression in sprintf()",
    ),
    t(Php, "switch", St, "switch («expr») {\n\t«end»\n}", SWITCH),
    t(Php, "throw", St, "throw «expr»;«end»", THROW),
    t(
        Php,
        "try",
        St,
        "try {\n\t«expr»;«end»\n} catch (\\Exception $e) {\n}",
        "Surrounds the statement with try/catch",
    ),
    t(
        Php,
        "unset",
        St,
        "unset(«expr»);«end»",
        "Destroys the variable",
    ),
    t(
        Php,
        "var",
        St,
        "«name» = «expr»;«end»",
        "Introduces a variable for the expression",
    ),
    t(Php, "while", St, "while («expr») {\n\t«end»\n}", WHILE),
    // JavaScript and TypeScript
    t(Js, "await", Ex, "await «expr»«end»", "Awaits the promise"),
    t(
        Js,
        "const",
        St,
        "const «name» = «expr»;«end»",
        "Introduces a constant for the expression",
    ),
    t(Js, "else", St, "if («not») {\n\t«end»\n}", IF_FALSE),
    t(
        Js,
        "for",
        St,
        "for (const «item» of «expr») {\n\t«end»\n}",
        "Iterates over the iterable",
    ),
    t(
        Js,
        "fori",
        St,
        "for (let i = 0; i < «expr»; i++) {\n\t«end»\n}",
        "Iterates with an index up to the expression",
    ),
    t(
        Js,
        "forin",
        St,
        "for (const key in «expr») {\n\t«end»\n}",
        "Iterates over the enumerable properties",
    ),
    t(
        Js,
        "forof",
        St,
        "for (const «item» of «expr») {\n\t«end»\n}",
        "Iterates over the iterable",
    ),
    t(Js, "if", St, "if («expr») {\n\t«end»\n}", IF_TRUE),
    t(
        Js,
        "let",
        St,
        "let «name» = «expr»;«end»",
        "Introduces a variable for the expression",
    ),
    t(
        Js,
        "log",
        St,
        "console.log(«expr»);«end»",
        "Logs the expression with console.log()",
    ),
    t(Js, "new", Cls, "new «expr»(«end»)", NEW),
    t(Js, "not", Ex, "«not»«end»", NOT),
    t(
        Js,
        "notnull",
        St,
        "if («expr» !== null) {\n\t«end»\n}",
        NOT_NULL,
    ),
    t(Js, "null", St, "if («expr» === null) {\n\t«end»\n}", NULL),
    t(Js, "par", Ex, "(«expr»)«end»", PAR),
    t(Js, "return", St, "return «expr»;«end»", RETURN),
    t(Js, "switch", St, "switch («expr») {\n\t«end»\n}", SWITCH),
    t(Js, "throw", St, "throw «expr»;«end»", THROW),
    t(
        Js,
        "typeof",
        Ex,
        "typeof «expr»«end»",
        "Gets the type of the expression",
    ),
    t(
        Js,
        "undefined",
        St,
        "if («expr» === undefined) {\n\t«end»\n}",
        "Checks expression to be undefined",
    ),
    t(
        Js,
        "var",
        St,
        "var «name» = «expr»;«end»",
        "Introduces a variable for the expression",
    ),
    t(Js, "while", St, "while («expr») {\n\t«end»\n}", WHILE),
    // Lua
    t(Lua, "else", St, "if «not» then\n\t«end»\nend", IF_FALSE),
    t(
        Lua,
        "for",
        St,
        "for i = 1, «expr» do\n\t«end»\nend",
        "Iterates with an index from 1 to the expression",
    ),
    t(Lua, "if", St, "if «expr» then\n\t«end»\nend", IF_TRUE),
    t(
        Lua,
        "ipairs",
        St,
        "for i, «item» in ipairs(«expr») do\n\t«end»\nend",
        "Iterates over the array part of the table",
    ),
    t(
        Lua,
        "local",
        St,
        "local «name» = «expr»«end»",
        "Introduces a local variable for the expression",
    ),
    t(
        Lua,
        "nil",
        St,
        "if «expr» == nil then\n\t«end»\nend",
        "Checks expression to be nil",
    ),
    t(Lua, "not", Ex, "«not»«end»", NOT),
    t(
        Lua,
        "notnil",
        St,
        "if «expr» ~= nil then\n\t«end»\nend",
        "Checks expression to be not nil",
    ),
    t(
        Lua,
        "pairs",
        St,
        "for key, value in pairs(«expr») do\n\t«end»\nend",
        "Iterates over all keys and values of the table",
    ),
    t(Lua, "par", Ex, "(«expr»)«end»", PAR),
    t(
        Lua,
        "print",
        St,
        "print(«expr»)«end»",
        "Prints the expression",
    ),
    t(Lua, "return", St, "return «expr»«end»", RETURN),
    t(Lua, "while", St, "while «expr» do\n\t«end»\nend", WHILE),
];

/// A template as Settings | Editor | General | Postfix Completion lists it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    /// `php.if`; the setting `editor.postfix.disabled` lists these ids.
    pub id: String,
    /// `if`, typed after the dot.
    pub key: String,
    pub language: Lang,
    /// "PHP", "JavaScript and TypeScript", "Lua".
    pub group: String,
    pub description: String,
    /// Example before expanding (`$expr.if`).
    pub before: String,
    /// The example expanded.
    pub after: String,
}

/// Every template, grouped by language and sorted by key.
pub fn templates() -> Vec<TemplateInfo> {
    let mut out: Vec<TemplateInfo> = TEMPLATES
        .iter()
        .map(|d| {
            let expr = example_expr(d);
            let after = render(d, &expr, None, "", "    ");
            TemplateInfo {
                id: template_id(d),
                key: d.key.to_string(),
                language: d.lang,
                group: d.lang.label().to_string(),
                description: d.description.to_string(),
                before: format!("{expr}.{}", d.key),
                after: after.text,
            }
        })
        .collect();
    out.sort_by(|a, b| (lang_order(a.language), &a.key).cmp(&(lang_order(b.language), &b.key)));
    out
}

fn lang_order(l: Lang) -> u8 {
    match l {
        Php => 0,
        Js => 1,
        Lua => 2,
    }
}

fn template_id(d: &Def) -> String {
    format!("{}.{}", d.lang.id(), d.key)
}

fn example_expr(d: &Def) -> String {
    match (d.scope, d.lang) {
        (Cls, _) => "Foo".into(),
        (_, Php) => "$expr".into(),
        _ => "expr".into(),
    }
}

/// What the UI sends: the current line up to the caret.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostfixRequest {
    /// Forge file type id (`php`, `typescript`, `lua`, …).
    pub language: String,
    /// Text of the caret's line before the caret.
    pub line: String,
    /// One indent unit (default four spaces).
    #[serde(default)]
    pub indent: Option<String>,
    /// Ids of switched-off templates (`php.echo`).
    #[serde(default)]
    pub disabled: Vec<String>,
}

/// One applicable template, expanded.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PostfixItem {
    pub id: String,
    pub key: String,
    pub description: String,
    /// Start of the replaced text in the line (UTF-16); the replacement ends at the caret.
    pub from: usize,
    /// The expression the template wraps (shown in the completion popup).
    pub expression: String,
    /// The replacement.
    pub text: String,
    /// Selection after expanding, relative to the start of `text` (UTF-16); equal for a caret.
    pub selection_from: usize,
    pub selection_to: usize,
}

/// The templates that apply at the caret.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PostfixCompletion {
    /// Start of the typed key after the dot (UTF-16 offset in the line).
    pub key_from: usize,
    /// The key typed so far (`fore`).
    pub key: String,
    pub items: Vec<PostfixItem>,
}

/// The postfix templates that apply to `expr.key|` at the end of `request.line`.
pub fn complete_postfix(request: &PostfixRequest) -> Option<PostfixCompletion> {
    let lang = Lang::from_file_type(&request.language)?;
    let chars: Vec<char> = request.line.chars().collect();
    let mut key_start = chars.len();
    while key_start > 0 && chars[key_start - 1].is_ascii_alphabetic() {
        key_start -= 1;
    }
    let dot = key_start.checked_sub(1)?;
    if chars[dot] != '.' || dot == 0 || chars[dot - 1].is_whitespace() {
        return None;
    }
    let classes = classify(&chars, lang, true);
    if classes[dot] != Class::Code {
        return None;
    }
    let tight = tight_start(&chars[..dot], &classes[..dot], lang);
    let statement = statement_start(&chars[..dot], &classes[..dot], lang);
    if tight.is_none() && statement.is_none() {
        return None;
    }
    let key: String = chars[key_start..].iter().collect();
    let unit = request
        .indent
        .clone()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "    ".into());
    let base_indent = indent_of(&request.line).to_string();
    let mut items = Vec::new();
    for d in TEMPLATES.iter().filter(|d| d.lang == lang) {
        let id = template_id(d);
        if request.disabled.contains(&id) {
            continue;
        }
        let start = match d.scope {
            St => statement,
            Ex => tight,
            Cls => tight.filter(|&s| is_class_name(&chars[s..dot], lang)),
        };
        let Some(start) = start else { continue };
        let expr: String = chars[start..dot]
            .iter()
            .collect::<String>()
            .trim_end()
            .to_string();
        let r = render(d, &expr, Some(lang), &base_indent, &unit);
        items.push(PostfixItem {
            id,
            key: d.key.to_string(),
            description: d.description.to_string(),
            from: u16_len_chars(&chars[..start]),
            expression: expr,
            text: r.text,
            selection_from: r.selection.0,
            selection_to: r.selection.1,
        });
    }
    if items.is_empty() {
        return None;
    }
    Some(PostfixCompletion {
        key_from: u16_len_chars(&chars[..key_start]),
        key,
        items,
    })
}

/// Start of the closest expression that ends at `chars.len()`: names, member accesses, calls,
/// indexes and literals (`$this->repo->find($id)`, `items[0].name`, `'text'`).
fn tight_start(chars: &[char], classes: &[Class], lang: Lang) -> Option<usize> {
    let mut i = chars.len();
    while i > 0 {
        let c = chars[i - 1];
        if classes[i - 1] == Class::Str {
            while i > 0 && classes[i - 1] == Class::Str {
                i -= 1;
            }
            continue;
        }
        if classes[i - 1] != Class::Code {
            break;
        }
        match c {
            ')' | ']' => i = matching_open(chars, classes, i - 1)?,
            c if lang.is_ident(c) || (lang == Php && c == '\\') => i -= 1,
            '>' if lang == Php && i >= 2 && chars[i - 2] == '-' => {
                i -= 2;
                if i > 0 && chars[i - 1] == '?' {
                    i -= 1;
                }
            }
            ':' if lang == Php && i >= 2 && chars[i - 2] == ':' => i -= 2,
            ':' if lang == Lua => i -= 1,
            '.' if lang != Php => {
                if i >= 2 && chars[i - 2] == '.' {
                    break;
                }
                i -= 1;
                if lang == Js && i > 0 && chars[i - 1] == '?' {
                    i -= 1;
                }
            }
            _ => break,
        }
    }
    // `new Foo()` is one expression.
    if lang != Lua && i >= 4 {
        let before: String = chars[i - 4..i].iter().collect();
        if before == "new " && (i == 4 || !lang.is_ident(chars[i - 5])) {
            i -= 4;
        }
    }
    if i >= chars.len() || matches!(chars[i], '.' | ':' | '-' | '?' | '>') {
        return None;
    }
    Some(i)
}

/// The `(` or `[` that `chars[close]` closes.
fn matching_open(chars: &[char], classes: &[Class], close: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut j = close + 1;
    while j > 0 {
        j -= 1;
        if classes[j] != Class::Code {
            continue;
        }
        match chars[j] {
            ')' | ']' | '}' => depth += 1,
            '(' | '[' | '{' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
    }
    None
}

/// Words that start statements but are not expressions.
fn starts_with_keyword(stmt: &[char], lang: Lang) -> bool {
    let word: String = stmt
        .iter()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    if stmt.get(word.len()).is_some_and(|&c| lang.is_ident(c)) {
        return false;
    }
    let keywords: &[&str] = match lang {
        Php => &[
            "return",
            "echo",
            "print",
            "if",
            "else",
            "elseif",
            "for",
            "foreach",
            "while",
            "do",
            "switch",
            "case",
            "default",
            "throw",
            "yield",
            "function",
            "fn",
            "class",
            "interface",
            "trait",
            "enum",
            "const",
            "use",
            "namespace",
            "public",
            "private",
            "protected",
            "static",
            "abstract",
            "final",
            "global",
            "try",
            "catch",
        ],
        Js => &[
            "return",
            "if",
            "else",
            "for",
            "while",
            "do",
            "switch",
            "case",
            "default",
            "throw",
            "yield",
            "function",
            "class",
            "const",
            "let",
            "var",
            "import",
            "export",
            "try",
            "catch",
            "interface",
            "type",
        ],
        Lua => &[
            "local", "return", "if", "elseif", "else", "for", "while", "repeat", "until",
            "function", "end", "do",
        ],
    };
    keywords.contains(&word.as_str())
}

/// Start of the statement that ends at `chars.len()`, if the text there is an expression
/// statement (not inside brackets, not after a keyword like `return`).
fn statement_start(chars: &[char], classes: &[Class], lang: Lang) -> Option<usize> {
    let mut boundary = 0;
    let mut depth = 0usize;
    let at = |i: usize, s: &str| {
        s.chars()
            .enumerate()
            .all(|(k, c)| chars.get(i + k) == Some(&c))
    };
    let mut i = 0;
    while i < chars.len() {
        if classes[i] != Class::Code {
            i += 1;
            continue;
        }
        let c = chars[i];
        if lang == Php && (at(i, "<?php") || at(i, "<?=") || at(i, "?>")) {
            let n = if at(i, "<?php") {
                5
            } else if at(i, "<?=") {
                3
            } else {
                2
            };
            boundary = i + n;
            depth = 0;
            i += n;
            continue;
        }
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                if depth == 0 {
                    // Closes a bracket of an earlier line: what follows is a continuation.
                    boundary = i + 1;
                } else {
                    depth -= 1;
                }
            }
            '{' | '}' => {
                boundary = i + 1;
                depth = 0;
            }
            ';' if depth == 0 => boundary = i + 1,
            _ if lang == Lua
                && depth == 0
                && c.is_ascii_alphabetic()
                && (i == 0 || !lang.is_ident(chars[i - 1])) =>
            {
                let word: String = chars[i..]
                    .iter()
                    .take_while(|c| lang.is_ident(**c))
                    .collect();
                if matches!(word.as_str(), "then" | "do" | "else" | "repeat") {
                    boundary = i + word.chars().count();
                }
                i += word.chars().count();
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    if depth != 0 {
        return None;
    }
    let mut start = boundary;
    while start < chars.len() && chars[start].is_whitespace() {
        start += 1;
    }
    let stmt = &chars[start..];
    if stmt.iter().all(|c| c.is_whitespace()) || starts_with_keyword(stmt, lang) {
        return None;
    }
    if matches!(stmt[0], '.' | ':' | '-' | '?' | '>' | '=' | ',') {
        return None;
    }
    Some(start)
}

fn is_class_name(expr: &[char], lang: Lang) -> bool {
    let Some(&first) = expr.first() else {
        return false;
    };
    (first.is_alphabetic() || first == '_' || (lang == Php && first == '\\'))
        && expr
            .iter()
            .all(|&c| c.is_alphanumeric() || c == '_' || (lang == Php && c == '\\'))
}

struct Rendered {
    text: String,
    /// UTF-16 offsets in `text`.
    selection: (usize, usize),
}

/// Expand a template body. `lang` is `None` for the settings examples, which use the
/// placeholder names `name` and `item`.
fn render(d: &Def, expr: &str, lang: Option<Lang>, base_indent: &str, unit: &str) -> Rendered {
    let sigil = if d.lang == Php { "$" } else { "" };
    let name = match lang {
        Some(l) => variable_name(expr, l).unwrap_or_else(|| "value".into()),
        None => "name".into(),
    };
    let item = match lang {
        Some(l) => variable_name(expr, l)
            .and_then(|n| singular(&n))
            .unwrap_or_else(|| "item".into()),
        None => "item".into(),
    };
    let mut text = String::new();
    let mut selection = None;
    let mut caret = None;
    let mut rest = d.body;
    while let Some(ch) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix('«') {
            let end = after.find('»').expect("closed marker");
            let marker = &after[..end];
            rest = &after[end + '»'.len_utf8()..];
            match marker {
                "expr" => text.push_str(expr),
                "not" => text.push_str(&negate(expr, d.lang)),
                "end" => caret = Some(u16_len(&text)),
                "name" | "item" => {
                    let from = u16_len(&text);
                    text.push_str(sigil);
                    text.push_str(if marker == "name" { &name } else { &item });
                    selection = Some((from, u16_len(&text)));
                }
                other => unreachable!("unknown marker {other}"),
            }
            continue;
        }
        match ch {
            '\n' => {
                text.push('\n');
                text.push_str(base_indent);
            }
            '\t' => text.push_str(unit),
            c => text.push(c),
        }
        rest = &rest[ch.len_utf8()..];
    }
    let end = u16_len(&text);
    let selection = selection.or(caret.map(|c| (c, c))).unwrap_or((end, end));
    Rendered { text, selection }
}

/// `!expr`, `!(a > b)`, or `expr` for `!expr`; Lua uses `not`.
fn negate(expr: &str, lang: Lang) -> String {
    let op = if lang == Lua { "not " } else { "!" };
    let simple = |e: &str| {
        let chars: Vec<char> = e.chars().collect();
        let classes = classify(&chars, lang, true);
        tight_start(&chars, &classes, lang) == Some(0)
    };
    if let Some(inner) = expr.strip_prefix(op) {
        if simple(inner.trim_start()) {
            return inner.trim_start().to_string();
        }
    }
    if simple(expr) && !expr.starts_with("new ") {
        format!("{op}{expr}")
    } else {
        format!("{op}({expr})")
    }
}

/// A variable name for the value of `expr`: `$this->getUser()` → `user`,
/// `new Order()` → `order`, `items[0]` → `items`.
fn variable_name(expr: &str, lang: Lang) -> Option<String> {
    let mut s: Vec<char> = expr.trim().chars().collect();
    if s.starts_with(&['n', 'e', 'w', ' ']) {
        s.drain(..4);
    }
    // Drop trailing calls and indexes.
    while let Some(')' | ']') = s.last() {
        let classes = classify(&s, lang, true);
        let open = matching_open(&s, &classes, s.len() - 1)?;
        s.truncate(open);
    }
    let mut start = s.len();
    while start > 0 && (s[start - 1].is_alphanumeric() || s[start - 1] == '_') {
        start -= 1;
    }
    let mut name: String = s[start..].iter().collect();
    for prefix in ["get", "find", "create", "load", "fetch"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            if rest.starts_with(|c: char| c.is_uppercase()) {
                name = rest.to_string();
                break;
            }
        }
    }
    let mut chars = name.chars();
    let first = chars.next()?;
    if !first.is_alphabetic() {
        return None;
    }
    let name: String = first.to_lowercase().chain(chars).collect();
    let reserved = [
        "this",
        "self",
        "static",
        "parent",
        "true",
        "false",
        "null",
        "nil",
        "undefined",
    ];
    (!reserved.contains(&name.as_str())).then_some(name)
}

/// `users` → `user`, `entries` → `entry`, `boxes` → `box`; `None` when not plural.
fn singular(name: &str) -> Option<String> {
    if let Some(stem) = name.strip_suffix("ies") {
        return (!stem.is_empty()).then(|| format!("{stem}y"));
    }
    for suffix in ["sses", "xes", "ches", "shes"] {
        if name.ends_with(suffix) {
            return Some(name[..name.len() - 2].to_string());
        }
    }
    let stem = name.strip_suffix('s')?;
    (!stem.is_empty() && !stem.ends_with('s')).then(|| stem.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete(language: &str, line: &str) -> Option<PostfixCompletion> {
        complete_postfix(&PostfixRequest {
            language: language.into(),
            line: line.into(),
            indent: Some("    ".into()),
            disabled: vec![],
        })
    }

    /// The line after applying template `key`, with `|` at the caret or `[…]` around the selection.
    fn expand(language: &str, line: &str, key: &str) -> String {
        let c = complete(language, line).unwrap_or_else(|| panic!("no completion for {line:?}"));
        let item = c
            .items
            .iter()
            .find(|i| i.key == key)
            .unwrap_or_else(|| panic!("no {key} for {line:?}"));
        let units: Vec<u16> = line.encode_utf16().collect();
        let head = String::from_utf16(&units[..item.from]).unwrap();
        let t: Vec<u16> = item.text.encode_utf16().collect();
        let a = String::from_utf16(&t[..item.selection_from]).unwrap();
        let b = String::from_utf16(&t[item.selection_from..item.selection_to]).unwrap();
        let rest = String::from_utf16(&t[item.selection_to..]).unwrap();
        if b.is_empty() {
            format!("{head}{a}|{rest}")
        } else {
            format!("{head}{a}[{b}]{rest}")
        }
    }

    fn keys(language: &str, line: &str) -> Vec<String> {
        complete(language, line)
            .map(|c| c.items.into_iter().map(|i| i.key).collect())
            .unwrap_or_default()
    }

    #[test]
    fn php_if_wraps_the_statement() {
        assert_eq!(
            expand("php", "    $user->isAdmin().if", "if"),
            "    if ($user->isAdmin()) {\n        |\n    }"
        );
        assert_eq!(expand("php", "$a > 0.if", "if"), "if ($a > 0) {\n    |\n}");
        assert_eq!(
            expand("php", "$a > 0.else", "else"),
            "if (!($a > 0)) {\n    |\n}"
        );
        assert_eq!(expand("php", "$ok.else", "else"), "if (!$ok) {\n    |\n}");
        assert_eq!(expand("php", "!$ok.else", "else"), "if ($ok) {\n    |\n}");
    }

    #[test]
    fn php_expression_templates_take_the_closest_expression() {
        assert_eq!(
            expand("php", "$x = $user->name.isset", "isset"),
            "$x = isset($user->name)|"
        );
        assert_eq!(
            expand("php", "foo($a, $b['k'].empty", "empty"),
            "foo($a, empty($b['k'])|"
        );
        assert_eq!(expand("php", "$ok = $valid.not", "not"), "$ok = !$valid|");
        assert_eq!(
            expand("php", "$s = 'Hi %s'.sprintf", "sprintf"),
            "$s = sprintf('Hi %s', |)"
        );
        assert_eq!(
            expand("php", "$o = \\App\\Order.new", "new"),
            "$o = new \\App\\Order(|)"
        );
        assert_eq!(
            expand("php", "Foo::create()->items.par", "par"),
            "(Foo::create()->items)|"
        );
    }

    #[test]
    fn php_loops_and_variables() {
        assert_eq!(
            expand("php", "    $this->getUsers().foreach", "foreach"),
            "    foreach ($this->getUsers() as [$user]) {\n        \n    }"
        );
        assert_eq!(
            expand("php", "$data.forek", "forek"),
            "foreach ($data as $key => [$item]) {\n    \n}"
        );
        assert_eq!(
            expand("php", "count($a).for", "for"),
            "for ($i = 0; $i < count($a); $i++) {\n    |\n}"
        );
        assert_eq!(
            expand("php", "$repo->findOrder($id).var", "var"),
            "[$order] = $repo->findOrder($id);"
        );
        assert_eq!(
            expand("php", "new Mailer().var", "var"),
            "[$mailer] = new Mailer();"
        );
        assert_eq!(expand("php", "$x.return", "return"), "return $x;|");
        assert_eq!(expand("php", "$e.throw", "throw"), "throw $e;|");
        assert_eq!(expand("php", "$name.echo", "echo"), "echo $name;|");
        assert_eq!(
            expand("php", "$v.null", "null"),
            "if ($v === null) {\n    |\n}"
        );
        assert_eq!(
            expand("php", "$v.notnull", "notnull"),
            "if ($v !== null) {\n    |\n}"
        );
        assert_eq!(
            expand("php", "$v.switch", "switch"),
            "switch ($v) {\n    |\n}"
        );
        assert_eq!(expand("php", "$v.while", "while"), "while ($v) {\n    |\n}");
        assert_eq!(expand("php", "$v.unset", "unset"), "unset($v);|");
    }

    #[test]
    fn statement_templates_need_a_statement() {
        // Inside a call, after a keyword or in a chain continuation only expression templates apply.
        let inside = keys("php", "foo($a.");
        assert!(inside.contains(&"not".to_string()) && !inside.contains(&"if".to_string()));
        assert!(!keys("php", "return $a.").contains(&"if".to_string()));
        assert!(keys("php", "if ($a) { $b.").contains(&"if".to_string()));
        assert!(keys("php", "<?php $b.").contains(&"if".to_string()));
        assert!(keys("php", "    ->where('a').").is_empty());
        // Class names only for .new
        assert!(!keys("php", "$a.").contains(&"new".to_string()));
        assert!(keys("php", "Foo.").contains(&"new".to_string()));
    }

    #[test]
    fn nothing_in_strings_comments_or_without_expression() {
        assert!(complete("php", "$s = 'a.if").is_none());
        assert!(complete("php", "// $a.if").is_none());
        assert!(complete("php", "    .if").is_none());
        assert!(complete("php", "$a .if").is_none());
        assert!(complete("markdown", "a.if").is_none());
        assert!(complete("js", "[...a").is_none());
    }

    #[test]
    fn reports_the_key_range_and_honours_disabled_templates() {
        let c = complete("php", "  $a.fo").unwrap();
        assert_eq!((c.key_from, c.key.as_str()), (5, "fo"));
        let off = complete_postfix(&PostfixRequest {
            language: "php".into(),
            line: "$a.".into(),
            indent: None,
            disabled: vec!["php.if".into()],
        })
        .unwrap();
        assert!(!off.items.iter().any(|i| i.key == "if"));
        assert!(off.items.iter().any(|i| i.key == "else"));
    }

    #[test]
    fn javascript_templates() {
        assert_eq!(
            expand("typescript", "  user?.name.if", "if"),
            "  if (user?.name) {\n      |\n  }"
        );
        assert_eq!(
            expand("javascript", "items.filter(x => x).forof", "forof"),
            "for (const [item] of items.filter(x => x)) {\n    \n}"
        );
        assert_eq!(
            expand("javascript", "this.orders.for", "for"),
            "for (const [order] of this.orders) {\n    \n}"
        );
        assert_eq!(
            expand("javascript", "fetchUser(id).const", "const"),
            "const [user] = fetchUser(id);"
        );
        assert_eq!(
            expand("javascript", "data.log", "log"),
            "console.log(data);|"
        );
        assert_eq!(
            expand("javascript", "const u = api.load().await", "await"),
            "const u = await api.load()|"
        );
        assert_eq!(expand("jsx", "a === b.not", "not"), "a === !b|");
        assert_eq!(
            expand("tsx", "a === b.else", "else"),
            "if (!(a === b)) {\n    |\n}"
        );
        assert_eq!(expand("javascript", "x.typeof", "typeof"), "typeof x|");
        assert_eq!(expand("javascript", "Map.new", "new"), "new Map(|)");
        assert_eq!(
            expand("javascript", "list.length.fori", "fori"),
            "for (let i = 0; i < list.length; i++) {\n    |\n}"
        );
        assert_eq!(
            expand("javascript", "`a ${b}`.log", "log"),
            "console.log(`a ${b}`);|"
        );
    }

    #[test]
    fn lua_templates() {
        assert_eq!(
            expand("lua", "  player.alive.if", "if"),
            "  if player.alive then\n      |\n  end"
        );
        assert_eq!(
            expand("lua", "ok.else", "else"),
            "if not ok then\n    |\nend"
        );
        assert_eq!(
            expand("lua", "a == b.else", "else"),
            "if not (a == b) then\n    |\nend"
        );
        assert_eq!(
            expand("lua", "#items.for", "for"),
            "for i = 1, #items do\n    |\nend"
        );
        assert_eq!(
            expand("lua", "self.enemies.ipairs", "ipairs"),
            "for i, [enemy] in ipairs(self.enemies) do\n    \nend"
        );
        assert_eq!(
            expand("lua", "config.pairs", "pairs"),
            "for key, value in pairs(config) do\n    |\nend"
        );
        assert_eq!(
            expand("lua", "obj:getName().local", "local"),
            "local [name] = obj:getName()"
        );
        assert_eq!(expand("lua", "msg.print", "print"), "print(msg)|");
        assert_eq!(
            expand("lua", "if ready then x.return", "return"),
            "if ready then return x|"
        );
        assert_eq!(expand("lua", "s = a..b.par", "par"), "s = a..(b)|");
    }

    #[test]
    fn utf16_offsets() {
        let c = complete("php", "$ä😀 = $x.if").unwrap();
        assert_eq!(c.key_from, 10);
        let not = c.items.iter().find(|i| i.key == "not").unwrap();
        assert_eq!(not.from, 7);
    }

    #[test]
    fn names_and_plurals() {
        assert_eq!(
            variable_name("$this->getUser()", Php).as_deref(),
            Some("user")
        );
        assert_eq!(variable_name("$a['x']", Php).as_deref(), Some("a"));
        assert_eq!(variable_name("'text'", Php), None);
        assert_eq!(variable_name("$this", Php), None);
        assert_eq!(singular("entries").as_deref(), Some("entry"));
        assert_eq!(singular("boxes").as_deref(), Some("box"));
        assert_eq!(singular("class"), None);
        assert_eq!(singular("data"), None);
    }

    #[test]
    fn lists_templates_with_examples() {
        let all = templates();
        let php_if = all.iter().find(|t| t.id == "php.if").unwrap();
        assert_eq!(php_if.before, "$expr.if");
        assert_eq!(php_if.after, "if ($expr) {\n    \n}");
        assert_eq!(php_if.group, "PHP");
        let var = all.iter().find(|t| t.id == "php.var").unwrap();
        assert_eq!(var.after, "$name = $expr;");
        assert_eq!(
            all.iter().find(|t| t.id == "js.new").unwrap().before,
            "Foo.new"
        );
        // Ids are unique; groups come in PHP, JS, Lua order.
        let mut ids: Vec<_> = all.iter().map(|t| t.id.clone()).collect();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
        assert_eq!(all.first().unwrap().language, Php);
        assert_eq!(all.last().unwrap().language, Lua);
    }
}
