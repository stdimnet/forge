//! Complete Current Statement (`Ctrl+Shift+Enter`, PHPStorm's `EditorCompleteStatement`):
//! closes the brackets the caret's line left open, adds the missing `;` (PHP, JavaScript,
//! TypeScript) or `then`/`do` (Lua), opens the block of control statements and function
//! declarations (`{ }` or `… end`) and puts the caret where typing continues. A line that is
//! already complete gets a new line below.

use serde::{Deserialize, Serialize};

use crate::lang::Lang;
use crate::text::{char_index, classify, indent_of, u16_len_chars, Class};

/// What the UI sends: the whole document and the caret.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementRequest {
    /// Forge file type id (`php`, `typescript`, `lua`, …).
    pub language: String,
    pub text: String,
    /// Caret (UTF-16 offset).
    pub offset: usize,
    /// One indent unit (default four spaces).
    #[serde(default)]
    pub indent: Option<String>,
}

/// One text change (UTF-16 offsets into the original text).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Change {
    pub from: usize,
    pub to: usize,
    pub insert: String,
}

/// The edit to apply; changes do not overlap and are sorted.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatementEdit {
    pub changes: Vec<Change>,
    /// Caret after the edit (UTF-16 offset into the new text).
    pub cursor: usize,
}

/// What kind of statement the caret's line holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Needs `;` (PHP, JS) or nothing (Lua).
    Simple,
    /// `if (…)`, `foreach (…)`, `else`: a block with the brace on the same line.
    Control,
    /// PHP function and class declarations: the brace goes on the next line (PSR-12).
    Declaration,
    /// Lua block statement: the word to add (`then`, `do`, or nothing) and whether it needs `end`.
    LuaBlock {
        opener: Option<&'static str>,
        end: Option<&'static str>,
    },
}

/// Complete the statement at `request.offset`; `None` for languages without support.
pub fn complete_statement(request: &StatementRequest) -> Option<StatementEdit> {
    let lang = Lang::from_file_type(&request.language)?;
    let unit = request
        .indent
        .clone()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "    ".into());
    let chars: Vec<char> = request.text.chars().collect();
    let caret = char_index(&chars, request.offset);
    let line_start = chars[..caret]
        .iter()
        .rposition(|&c| c == '\n')
        .map_or(0, |i| i + 1);
    let line_end = chars[caret..]
        .iter()
        .position(|&c| c == '\n')
        .map_or(chars.len(), |i| caret + i);
    let starts_in_code = lang != Lang::Php || !request.text.contains("<?");
    let classes = classify(&chars, lang, starts_in_code);
    let line: String = chars[line_start..line_end].iter().collect();
    let indent = indent_of(&line).to_string();

    let mut edit = Builder {
        chars: &chars,
        changes: Vec::new(),
        cursor: None,
    };

    // The last code character of the line; insertions go after it (before a trailing comment).
    let code_end = (line_start..line_end)
        .rev()
        .find(|&i| classes[i] != Class::Comment && !chars[i].is_whitespace())
        .map(|i| i + 1);
    let Some(code_end) =
        code_end.filter(|&e| classes[e - 1] == Class::Code || classes[e - 1] == Class::Str)
    else {
        return Some(edit.new_line(line_end, &indent));
    };
    let code_start = (line_start..code_end)
        .find(|&i| !chars[i].is_whitespace())
        .unwrap_or(code_end);

    // Brackets the line leaves open, innermost last.
    let mut open: Vec<(char, usize)> = Vec::new();
    for i in code_start..code_end {
        if classes[i] != Class::Code {
            continue;
        }
        match chars[i] {
            '(' | '[' | '{' => open.push((chars[i], i)),
            ')' | ']' | '}' => {
                let want = match chars[i] {
                    ')' => '(',
                    ']' => '[',
                    _ => '{',
                };
                if open.last().is_some_and(|(c, _)| *c == want) {
                    open.pop();
                }
            }
            _ => {}
        }
    }
    // A block opened on this line (`function () {`, `if ($a) {`): close what was opened
    // before it after the block and go inside.
    if let Some(brace) = open.iter().rposition(|(c, _)| *c == '{') {
        let tail: String = open[..brace]
            .iter()
            .rev()
            .map(|(c, _)| closer(*c))
            .collect();
        let inner: String = open[brace + 1..]
            .iter()
            .rev()
            .map(|(c, _)| closer(*c))
            .collect();
        let pos = open[brace].1 + 1;
        // Brackets opened after the brace belong inside the block; close them there.
        if !inner.is_empty() {
            edit.insert(code_end, &inner);
        }
        let semicolon = if lang != Lang::Lua && !tail.is_empty() {
            ";"
        } else {
            ""
        };
        let closed = block_closed(&chars, line_end, &indent, &["}"]);
        let body = format!("\n{indent}{unit}");
        if inner.is_empty() && pos == code_end {
            if closed {
                return Some(edit.with_cursor_after(code_end, &body));
            }
            return Some(edit.wrap(code_end, &body, &format!("\n{indent}}}{tail}{semicolon}")));
        }
        return Some(edit.new_line(line_end, &indent));
    }
    let closers: String = open.iter().rev().map(|(c, _)| closer(*c)).collect();
    let code: String = chars[code_start..code_end].iter().collect();
    let kind = kind_of(&code, lang);
    let next = next_code_line(&chars, line_end);

    match kind {
        Kind::Control | Kind::Declaration => {
            let ends_with = |s: &str| code.trim_end().ends_with(s);
            if ends_with(";") {
                return Some(edit.new_line(line_end, &indent));
            }
            edit.insert(code_end, &closers);
            // The brace is already on the next line (PSR-12 style): go into the block.
            if let Some((next_start, next_end)) = next {
                let next_text: String = chars[next_start..next_end].iter().collect();
                if next_text.trim() == "{" {
                    let next_indent = indent_of(&next_text).to_string();
                    return Some(
                        edit.with_cursor_after(next_end, &format!("\n{next_indent}{unit}")),
                    );
                }
            }
            let brace = if kind == Kind::Declaration && lang == Lang::Php {
                format!("\n{indent}{{")
            } else {
                " {".to_string()
            };
            Some(edit.wrap(
                code_end,
                &format!("{brace}\n{indent}{unit}"),
                &format!("\n{indent}}}"),
            ))
        }
        Kind::LuaBlock { opener, end } => {
            edit.insert(code_end, &closers);
            let has_opener = opener.is_some_and(|w| ends_with_word(&code, w));
            let opener = match opener {
                Some(w) if !has_opener => format!(" {w}"),
                _ => String::new(),
            };
            let body = format!("{opener}\n{indent}{unit}");
            match end {
                Some(end)
                    if !block_closed(
                        &chars,
                        line_end,
                        &indent,
                        &["end", "else", "elseif", "until"],
                    ) =>
                {
                    Some(edit.wrap(code_end, &body, &format!("\n{indent}{end}")))
                }
                _ => Some(edit.with_cursor_after(code_end, &body)),
            }
        }
        Kind::Simple => {
            let mut add = closers.clone();
            if lang != Lang::Lua
                && needs_semicolon(
                    &chars,
                    &classes,
                    line_start,
                    &format!("{code}{closers}"),
                    next,
                    lang,
                )
            {
                add.push(';');
            }
            if add.is_empty() {
                return Some(edit.new_line(line_end, &indent));
            }
            edit.insert(code_end, &add);
            Some(edit.with_cursor_after(line_end, ""))
        }
    }
}

fn closer(open: char) -> char {
    match open {
        '(' => ')',
        '[' => ']',
        _ => '}',
    }
}

/// The kind of statement by its first words.
fn kind_of(raw: &str, lang: Lang) -> Kind {
    let code = raw.trim_start_matches(|c: char| c == '}' || c.is_whitespace());
    let words: Vec<&str> = code
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .take(6)
        .collect();
    let first = words.first().copied().unwrap_or("");
    let starts_with_word = |w: &str| {
        code.starts_with(w)
            && !code[w.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
    };
    let first_is = |w: &str| first == w && starts_with_word(w);
    match lang {
        Lang::Lua => {
            if first_is("if") || first_is("elseif") {
                let end = if first == "if" { Some("end") } else { None };
                Kind::LuaBlock {
                    opener: Some("then"),
                    end,
                }
            } else if first_is("for") || first_is("while") {
                Kind::LuaBlock {
                    opener: Some("do"),
                    end: Some("end"),
                }
            } else if first_is("else") {
                Kind::LuaBlock {
                    opener: None,
                    end: None,
                }
            } else if first_is("repeat") {
                Kind::LuaBlock {
                    opener: None,
                    end: Some("until "),
                }
            } else if first_is("do")
                || (words.contains(&"function") && !ends_with_word(code, "end"))
            {
                Kind::LuaBlock {
                    opener: None,
                    end: Some("end"),
                }
            } else {
                Kind::Simple
            }
        }
        Lang::Php | Lang::Js => {
            const CONTROL: &[&str] = &[
                "if", "elseif", "else", "for", "foreach", "while", "switch", "catch", "finally",
                "try", "do",
            ];
            const MODIFIERS: &[&str] = &[
                "public",
                "private",
                "protected",
                "static",
                "final",
                "abstract",
                "readonly",
                "async",
                "export",
                "default",
            ];
            const DECLARATIONS: &[&str] = &["function", "class", "interface", "trait", "enum"];
            if CONTROL.contains(&first) && starts_with_word(first) {
                // `} while ($x)` ends a do-while loop.
                let ends_do = first == "while" && raw.trim_start().starts_with('}');
                return if ends_do { Kind::Simple } else { Kind::Control };
            }
            let decl = words.iter().find(|w| !MODIFIERS.contains(w));
            if decl.is_some_and(|w| DECLARATIONS.contains(w)) && !words.contains(&"abstract") {
                return if lang == Lang::Php {
                    Kind::Declaration
                } else {
                    Kind::Control
                };
            }
            Kind::Simple
        }
    }
}

fn ends_with_word(code: &str, word: &str) -> bool {
    let code = code.trim_end();
    code.ends_with(word)
        && !code[..code.len() - word.len()].ends_with(|c: char| c.is_alphanumeric() || c == '_')
}

/// Whether a simple statement needs a `;`.
fn needs_semicolon(
    chars: &[char],
    classes: &[Class],
    line_start: usize,
    code: &str,
    next: Option<(usize, usize)>,
    lang: Lang,
) -> bool {
    let code = code.trim_end();
    let last = code.chars().last().unwrap_or(';');
    if matches!(
        last,
        ';' | '{'
            | '}'
            | ','
            | ':'
            | '('
            | '['
            | '.'
            | '+'
            | '-'
            | '*'
            | '/'
            | '='
            | '&'
            | '|'
            | '?'
            | '>'
            | '<'
    ) && !code.ends_with("++")
        && !code.ends_with("--")
    {
        return false;
    }
    if code.starts_with('@')
        || code.starts_with("#[")
        || code.starts_with("<?")
        || code.ends_with("?>")
    {
        return false;
    }
    // Inside a multi-line call, array or object literal.
    if !in_block(chars, classes, line_start, lang) {
        return false;
    }
    // The statement continues on the next line (`->where(…)`, `.then(…)`, `+ $b`).
    if let Some((s, e)) = next {
        let text: String = chars[s..e].iter().collect();
        let t = text.trim_start();
        if [
            "->", "?->", ".", "?.", "+", "-", "*", "/", "&&", "||", "?", ":", "=", "??",
        ]
        .iter()
        .any(|p| t.starts_with(p))
        {
            return false;
        }
    }
    true
}

/// Whether the line at `line_start` is directly inside a block (or at the top level) rather
/// than inside brackets or a JavaScript object literal.
fn in_block(chars: &[char], classes: &[Class], line_start: usize, lang: Lang) -> bool {
    // Each open bracket and whether it opens a block.
    let mut stack: Vec<(char, bool)> = Vec::new();
    let mut prev = None::<char>;
    for i in 0..line_start {
        if classes[i] != Class::Code {
            continue;
        }
        let c = chars[i];
        match c {
            '(' | '[' => stack.push((c, false)),
            '{' => {
                let literal = lang == Lang::Js
                    && matches!(prev, Some('=' | '(' | ',' | ':' | '[' | '?') | None);
                stack.push((c, !literal));
            }
            ')' | ']' | '}' => {
                stack.pop();
            }
            _ => {}
        }
        if !c.is_whitespace() {
            prev = Some(c);
        }
    }
    stack.last().is_none_or(|(c, block)| *c == '{' && *block)
}

/// Start and end of the next line with something other than whitespace.
fn next_code_line(chars: &[char], line_end: usize) -> Option<(usize, usize)> {
    let mut start = line_end + 1;
    while start <= chars.len() {
        let end = chars[start..]
            .iter()
            .position(|&c| c == '\n')
            .map_or(chars.len(), |i| start + i);
        if chars[start..end].iter().any(|c| !c.is_whitespace()) {
            return Some((start, end));
        }
        start = end + 1;
    }
    None
}

/// Whether the block opened on the line ending at `line_end` already has its closing line:
/// the next line indented like the statement starts with one of `closers`.
fn block_closed(chars: &[char], line_end: usize, indent: &str, closers: &[&str]) -> bool {
    let mut at = line_end;
    while let Some((s, e)) = next_code_line(chars, at) {
        let text: String = chars[s..e].iter().collect();
        let own = indent_of(&text);
        if own.len() <= indent.len() {
            return own == indent && closers.iter().any(|c| text.trim_start().starts_with(c));
        }
        at = e;
    }
    false
}

struct Builder<'a> {
    chars: &'a [char],
    /// (char position, inserted text), in position order.
    changes: Vec<(usize, String)>,
    /// Caret as (char position in the original, text inserted at that position before it).
    cursor: Option<(usize, String)>,
}

impl Builder<'_> {
    fn insert(&mut self, at: usize, text: &str) {
        if text.is_empty() {
            return;
        }
        match self.changes.last_mut() {
            Some((pos, existing)) if *pos == at => existing.push_str(text),
            _ => self.changes.push((at, text.to_string())),
        }
    }

    /// Insert `before` and `after` at `at` with the caret between them.
    fn wrap(mut self, at: usize, before: &str, after: &str) -> StatementEdit {
        self.insert(at, before);
        let prefix = self.inserted_at(at);
        self.insert(at, after);
        self.cursor = Some((at, prefix));
        self.finish()
    }

    /// Insert `text` at `at` and put the caret after it.
    fn with_cursor_after(mut self, at: usize, text: &str) -> StatementEdit {
        self.insert(at, text);
        let prefix = self.inserted_at(at);
        self.cursor = Some((at, prefix));
        self.finish()
    }

    /// A new line below the line ending at `line_end`.
    fn new_line(self, line_end: usize, indent: &str) -> StatementEdit {
        self.with_cursor_after(line_end, &format!("\n{indent}"))
    }

    fn inserted_at(&self, at: usize) -> String {
        self.changes
            .iter()
            .find(|(p, _)| *p == at)
            .map(|(_, t)| t.clone())
            .unwrap_or_default()
    }

    fn finish(self) -> StatementEdit {
        let (pos, prefix) = self.cursor.unwrap_or((self.chars.len(), String::new()));
        let shift: usize = self
            .changes
            .iter()
            .filter(|(p, _)| *p < pos)
            .map(|(_, t)| t.encode_utf16().count())
            .sum();
        let cursor = u16_len_chars(&self.chars[..pos]) + shift + prefix.encode_utf16().count();
        let changes = self
            .changes
            .into_iter()
            .map(|(p, insert)| {
                let at = u16_len_chars(&self.chars[..p]);
                Change {
                    from: at,
                    to: at,
                    insert,
                }
            })
            .collect();
        StatementEdit { changes, cursor }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply Complete Current Statement to `src` (caret at `|`) and show the caret as `|`.
    fn run(language: &str, src: &str) -> String {
        let offset = src.find('|').expect("caret");
        let text = src.replacen('|', "", 1);
        let units: Vec<u16> = text.encode_utf16().collect();
        let offset = text[..offset].encode_utf16().count();
        let edit = complete_statement(&StatementRequest {
            language: language.into(),
            text: text.clone(),
            offset,
            indent: Some("    ".into()),
        })
        .expect("edit");
        let mut out: Vec<u16> = Vec::new();
        let mut last = 0;
        for c in &edit.changes {
            out.extend_from_slice(&units[last..c.from]);
            out.extend(c.insert.encode_utf16());
            last = c.to;
        }
        out.extend_from_slice(&units[last..]);
        let mut s = String::from_utf16(&out[..edit.cursor]).unwrap();
        s.push('|');
        s.push_str(&String::from_utf16(&out[edit.cursor..]).unwrap());
        s
    }

    #[test]
    fn php_adds_semicolon_and_parens() {
        assert_eq!(
            run("php", "<?php\n$a = foo(1, bar(2|"),
            "<?php\n$a = foo(1, bar(2));|"
        );
        assert_eq!(run("php", "$x = [1, 2|"), "$x = [1, 2];|");
        assert_eq!(
            run("php", "    echo $a| // note\n"),
            "    echo $a; // note|\n"
        );
        assert_eq!(run("php", "$i++|"), "$i++;|");
    }

    #[test]
    fn complete_lines_get_a_new_line() {
        assert_eq!(run("php", "    $a = 1;|\n"), "    $a = 1;\n    |\n");
        assert_eq!(
            run("php", "  // just a comment|"),
            "  // just a comment\n  |"
        );
        assert_eq!(run("javascript", "x = 1;|"), "x = 1;\n|");
    }

    #[test]
    fn php_control_statements_get_a_block() {
        assert_eq!(
            run("php", "    if ($a > 0|"),
            "    if ($a > 0) {\n        |\n    }"
        );
        assert_eq!(
            run("php", "foreach ($items as $item|"),
            "foreach ($items as $item) {\n    |\n}"
        );
        assert_eq!(run("php", "} else|"), "} else {\n    |\n}");
        assert_eq!(run("php", "} elseif ($b|"), "} elseif ($b) {\n    |\n}");
        assert_eq!(run("php", "while (true|"), "while (true) {\n    |\n}");
        assert_eq!(run("php", "} while ($x|"), "} while ($x);|");
    }

    #[test]
    fn php_declarations_put_the_brace_on_the_next_line() {
        assert_eq!(
            run("php", "    public function handle(Request $r|"),
            "    public function handle(Request $r)\n    {\n        |\n    }"
        );
        assert_eq!(
            run("php", "final class Order|"),
            "final class Order\n{\n    |\n}"
        );
        assert_eq!(
            run("php", "    abstract public function x(|"),
            "    abstract public function x();|"
        );
        // The brace is already there.
        assert_eq!(
            run("php", "    public function a(|\n    {\n    }"),
            "    public function a()\n    {\n        |\n    }"
        );
    }

    #[test]
    fn opened_blocks_get_closed_or_entered() {
        assert_eq!(run("php", "if ($a) {|"), "if ($a) {\n    |\n}");
        assert_eq!(run("php", "if ($a) {|\n}"), "if ($a) {\n    |\n}");
        assert_eq!(
            run("javascript", "  items.forEach((item) => {|"),
            "  items.forEach((item) => {\n      |\n  });"
        );
    }

    #[test]
    fn no_semicolon_inside_multiline_expressions() {
        assert_eq!(
            run("php", "$a = [\n    'x' => foo(1|\n];"),
            "$a = [\n    'x' => foo(1)|\n];"
        );
        assert_eq!(
            run("php", "$q = $db\n    ->where('a', 1|\n    ->get();"),
            "$q = $db\n    ->where('a', 1)|\n    ->get();"
        );
        assert_eq!(
            run("javascript", "const o = {\n  a: f(1|\n};"),
            "const o = {\n  a: f(1)|\n};"
        );
        assert_eq!(
            run("javascript", "function f() {\n  return g(1|\n}"),
            "function f() {\n  return g(1);|\n}"
        );
    }

    #[test]
    fn javascript_statements() {
        assert_eq!(
            run("typescript", "const x = load(\"a\"|"),
            "const x = load(\"a\");|"
        );
        assert_eq!(run("javascript", "if (a && b|"), "if (a && b) {\n    |\n}");
        assert_eq!(
            run("javascript", "function go(a, b|"),
            "function go(a, b) {\n    |\n}"
        );
        assert_eq!(
            run("javascript", "export class Store|"),
            "export class Store {\n    |\n}"
        );
        assert_eq!(run("javascript", "@Component({})|"), "@Component({})\n|");
    }

    #[test]
    fn lua_blocks() {
        assert_eq!(run("lua", "if x > 1|"), "if x > 1 then\n    |\nend");
        assert_eq!(run("lua", "  if ok then|"), "  if ok then\n      |\n  end");
        assert_eq!(run("lua", "if ok then|\nend"), "if ok then\n    |\nend");
        assert_eq!(run("lua", "elseif y|"), "elseif y then\n    |");
        assert_eq!(run("lua", "for i = 1, 10|"), "for i = 1, 10 do\n    |\nend");
        assert_eq!(
            run("lua", "for k, v in pairs(t|"),
            "for k, v in pairs(t) do\n    |\nend"
        );
        assert_eq!(run("lua", "while running|"), "while running do\n    |\nend");
        assert_eq!(
            run("lua", "local function greet(name|"),
            "local function greet(name)\n    |\nend"
        );
        assert_eq!(run("lua", "repeat|"), "repeat\n    |\nuntil ");
        assert_eq!(run("lua", "print(\"hi\"|"), "print(\"hi\")|");
        assert_eq!(run("lua", "x = 1|"), "x = 1\n|");
    }

    #[test]
    fn unsupported_languages() {
        assert!(complete_statement(&StatementRequest {
            language: "markdown".into(),
            text: "a".into(),
            offset: 1,
            indent: None
        })
        .is_none());
    }

    #[test]
    fn utf16_offsets() {
        assert_eq!(run("php", "$s = '😀'|"), "$s = '😀';|");
        assert_eq!(run("php", "$s = f('😀'|"), "$s = f('😀');|");
    }
}
