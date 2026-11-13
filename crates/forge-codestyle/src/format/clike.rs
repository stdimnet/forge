//! The formatter of brace languages (PHP, JavaScript/TypeScript, CSS, JSON) and Lua.
//!
//! - [`Mode::Layout`] lays a text out completely: indentation from the nesting, brace
//!   placement, spaces, blank lines and conversions. It formats the previews.
//! - [`Mode::Spacing`] keeps the lines and their indentation and only sets the spaces
//!   between tokens on a line, the brace and `else`/`catch`/`finally` placement and the
//!   conversions. It runs after php-lsp formatted a PHP file.
//! - [`Mode::Passes`] only applies the conversions (quotes, hex colors).

use super::lexer::{join, lex, Kind, Lang, Tok};
use super::{Indent, Opts};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Layout,
    Spacing,
    Passes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fk {
    // Parentheses.
    Call,
    Decl,
    Control,
    For,
    Declare,
    Group,
    // Brackets.
    Array,
    Index,
    Attr,
    // Braces.
    Class,
    Function,
    Closure,
    Block,
    Object,
    Match,
    Switch,
    Rule,
    Interp,
    // Lua `function`/`then`/`do`/`repeat` blocks.
    Lua,
}

impl Fk {
    fn is_block(self) -> bool {
        matches!(
            self,
            Fk::Class | Fk::Function | Fk::Closure | Fk::Block | Fk::Switch | Fk::Rule
        )
    }
}

#[derive(Debug, Clone)]
struct Frame {
    kind: Fk,
    /// Indentation (columns) of the line the frame opened on.
    base: usize,
    /// Indentation of the frame's contents.
    inner: usize,
    /// Output line the frame opened on.
    line: usize,
    in_case: bool,
    ternary: usize,
}

const PHP_NOT_OPERAND: &[&str] = &[
    "return",
    "echo",
    "print",
    "case",
    "and",
    "or",
    "xor",
    "instanceof",
    "new",
    "clone",
    "yield",
    "throw",
    "else",
    "include",
    "include_once",
    "require",
    "require_once",
    "as",
    "in",
    "use",
    "function",
    "fn",
    "if",
    "elseif",
    "while",
    "for",
    "foreach",
    "switch",
    "match",
    "catch",
    "default",
    "do",
    "goto",
    "insteadof",
    "extends",
    "implements",
    "const",
    "public",
    "protected",
    "private",
    "static",
    "abstract",
    "final",
    "readonly",
    "var",
    "global",
    "namespace",
    "class",
    "interface",
    "trait",
    "enum",
    "typeof",
    "void",
    "delete",
    "await",
    "of",
    "let",
    "local",
    "not",
    "then",
    "elseif",
    "until",
    "export",
    "import",
    "from",
];

const LUA_KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

const CONTROL: &[&str] = &[
    "if", "elseif", "while", "switch", "match", "catch", "with", "until",
];

struct F<'a> {
    lang: Lang,
    mode: Mode,
    o: &'a Opts<'a>,
    ind: Indent,
    t: Vec<Tok>,
    out: String,
    /// Lines written so far.
    lines: usize,
    frames: Vec<Frame>,
    /// Indentation (columns) of the current output line.
    line_cols: usize,
    /// Spacing mode: the whitespace that indents the current line.
    line_ws: String,
    /// Index of the last emitted token that is not a comment.
    prev: Option<usize>,
    /// Index of the last emitted token.
    last: Option<usize>,
    /// The frame kind a closing token closed, and whether it spanned lines.
    closed: Vec<Option<(Fk, bool)>>,
    at_stmt_start: bool,
    stmt_cols: usize,
    stmt_ws: String,
    stmt_word: String,
    last_stmt_word: String,
    /// A parenthesis of the current statement spanned lines (PSR-12 `) {`).
    stmt_wrapped_paren: bool,
    /// The token is a ternary `:`.
    ternary_colon: Vec<bool>,
    /// The `(` that was opened by each token (for the spacing of its contents).
    open_kind: Vec<Option<Fk>>,
}

/// Format `src`.
pub fn format(src: &str, lang: Lang, o: &Opts<'_>, mode: Mode) -> String {
    let mut t = lex(src, lang);
    passes(&mut t, lang, o, mode);
    if mode == Mode::Passes {
        return join(&t);
    }
    let n = t.len();
    let mut f = F {
        lang,
        mode,
        o,
        ind: o.indent(),
        t,
        out: String::with_capacity(src.len() + src.len() / 4),
        lines: 0,
        frames: vec![Frame {
            kind: Fk::Block,
            base: 0,
            inner: 0,
            line: 0,
            in_case: false,
            ternary: 0,
        }],
        line_cols: 0,
        line_ws: String::new(),
        prev: None,
        last: None,
        closed: vec![None; n],
        at_stmt_start: true,
        stmt_cols: 0,
        stmt_ws: String::new(),
        stmt_word: String::new(),
        last_stmt_word: String::new(),
        stmt_wrapped_paren: false,
        ternary_colon: vec![false; n],
        open_kind: vec![None; n],
    };
    for i in 0..n {
        f.emit(i);
    }
    let mut out = f.out;
    if mode == Mode::Layout {
        // No trailing spaces; one final line break.
        out = out
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n");
        out = out.trim_end().to_string() + "\n";
    }
    out
}

// ---- Token passes ------------------------------------------------------------------------

fn passes(t: &mut Vec<Tok>, lang: Lang, o: &Opts<'_>, mode: Mode) {
    match lang {
        Lang::Php => {
            match o.str("codeConversion.arraySyntax").as_str() {
                "short" => php_short_arrays(t),
                "long" => php_long_arrays(t),
                _ => {}
            }
            if o.bool("codeConversion.trailingCommaInMultilineArrays") {
                php_trailing_commas(t);
            }
            let case = o.str("codeConversion.constantsCase");
            if case == "lower" || case == "upper" {
                php_constants(t, case == "upper");
            }
        }
        Lang::Js | Lang::Ts => {
            match o.str("punctuation.quotes").as_str() {
                "single" => quotes(t, '\'', false),
                "double" => quotes(t, '"', false),
                _ => {}
            }
            if mode == Mode::Layout {
                match o.str("punctuation.semicolons").as_str() {
                    "use" => js_add_semicolons(t),
                    "dontUse" => js_remove_semicolons(t),
                    _ => {}
                }
            }
        }
        Lang::Css => match o.str("other.hexColorCase").as_str() {
            "lower" => hex_case(t, false),
            "upper" => hex_case(t, true),
            _ => {}
        },
        Lang::Lua if mode == Mode::Layout => {
            match o.str("other.quoteStyle").as_str() {
                "forceDouble" => quotes(t, '"', false),
                "forceSingle" => quotes(t, '\'', false),
                "autoPreferSingle" => quotes(t, '\'', true),
                _ => quotes(t, '"', true),
            }
            lua_call_parens(t, &o.str("other.callParentheses"));
            lua_collapse(t, &o.str("wrapping.collapseSimpleStatements"));
        }
        _ => {}
    }
}

/// Index of the token closing the bracket opened at `open`.
fn matching(t: &[Tok], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, tok) in t.iter().enumerate().skip(open) {
        match tok.kind {
            Kind::Open => depth += 1,
            Kind::Close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn prev_sig(t: &[Tok], i: usize) -> Option<usize> {
    (0..i)
        .rev()
        .find(|&j| t[j].kind != Kind::Comment && !t[j].text.is_empty())
}

fn next_sig(t: &[Tok], i: usize) -> Option<usize> {
    (i + 1..t.len()).find(|&j| t[j].kind != Kind::Comment && !t[j].text.is_empty())
}

fn is_operand(t: &Tok) -> bool {
    match t.kind {
        Kind::Var | Kind::Num | Kind::Str | Kind::Close | Kind::Cast => t.kind != Kind::Cast,
        Kind::Word => !PHP_NOT_OPERAND
            .iter()
            .any(|k| t.text.eq_ignore_ascii_case(k)),
        _ => false,
    }
}

/// `[` that starts an array literal (not an index or a destructuring assignment).
fn php_array_literal(t: &[Tok], i: usize) -> bool {
    if !t[i].is("[") {
        return false;
    }
    if let Some(p) = prev_sig(t, i) {
        let p = &t[p];
        if is_operand(p) || p.is_word("as") {
            return false;
        }
    }
    let Some(close) = matching(t, i) else {
        return false;
    };
    // `[$a, $b] = …` destructures.
    !next_sig(t, close).is_some_and(|n| t[n].is_op("="))
}

fn php_long_array_call(t: &[Tok], i: usize) -> bool {
    t[i].is_word("array")
        && t.get(i + 1)
            .is_some_and(|n| n.is("(") && n.kind == Kind::Open)
        && !prev_sig(t, i).is_some_and(|p| {
            t[p].is_op("->") || t[p].is_op("?->") || t[p].is_op("::") || t[p].is_word("function")
        })
}

fn php_short_arrays(t: &mut Vec<Tok>) {
    let mut i = 0;
    while i < t.len() {
        if php_long_array_call(t, i) {
            if let Some(close) = matching(t, i + 1) {
                t[close].text = "]".into();
                let ws = std::mem::take(&mut t[i].ws);
                t.remove(i);
                t[i].text = "[".into();
                t[i].ws = ws;
            }
        }
        i += 1;
    }
}

fn php_long_arrays(t: &mut Vec<Tok>) {
    let mut i = 0;
    while i < t.len() {
        if php_array_literal(t, i) {
            if let Some(close) = matching(t, i) {
                t[close].text = ")".into();
                let ws = std::mem::take(&mut t[i].ws);
                t[i].text = "(".into();
                t.insert(i, Tok::new(Kind::Word, "array", &ws));
                i += 1;
            }
        }
        i += 1;
    }
}

fn php_trailing_commas(t: &mut Vec<Tok>) {
    let mut i = 0;
    while i < t.len() {
        let open = if php_array_literal(t, i) {
            Some(i)
        } else if php_long_array_call(t, i) {
            Some(i + 1)
        } else {
            None
        };
        if let Some(open) = open {
            if let Some(close) = matching(t, open) {
                if t[close].nl() > 0 {
                    if let Some(last) = prev_sig(t, close) {
                        if last > open && !t[last].is(",") {
                            t.insert(last + 1, Tok::new(Kind::Op, ",", ""));
                        }
                    }
                }
            }
        }
        i += 1;
    }
}

fn php_constants(t: &mut [Tok], upper: bool) {
    for i in 0..t.len() {
        if t[i].kind != Kind::Word {
            continue;
        }
        let (prefix, name) = match t[i].text.strip_prefix('\\') {
            Some(n) => ("\\", n),
            None => ("", t[i].text.as_str()),
        };
        if !["true", "false", "null"]
            .iter()
            .any(|c| c.eq_ignore_ascii_case(name))
        {
            continue;
        }
        let member = prev_sig(t, i).is_some_and(|p| {
            t[p].is_op("->")
                || t[p].is_op("?->")
                || t[p].is_op("::")
                || t[p].is_word("const")
                || t[p].is_word("function")
                || t[p].is_word("case")
        });
        let call = t.get(i + 1).is_some_and(|n| n.is("(") || n.is_op("::"));
        if member || call {
            continue;
        }
        let name = if upper {
            name.to_ascii_uppercase()
        } else {
            name.to_ascii_lowercase()
        };
        t[i].text = format!("{prefix}{name}");
    }
}

/// Convert quoted strings to `quote`. With `prefer`, only strings without that quote change.
fn quotes(t: &mut [Tok], quote: char, prefer: bool) {
    let other = if quote == '"' { '\'' } else { '"' };
    for tok in t.iter_mut().filter(|t| t.kind == Kind::Str) {
        let s = &tok.text;
        if s.len() < 2 || !s.starts_with(other) || !s.ends_with(other) {
            continue;
        }
        let inner = &s[1..s.len() - 1];
        if inner.contains('\n') || (prefer && inner.contains(quote)) {
            continue;
        }
        let mut out = String::with_capacity(inner.len() + 2);
        out.push(quote);
        let mut chars = inner.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some(n) if n == other => out.push(n),
                    Some(n) => {
                        out.push('\\');
                        out.push(n);
                    }
                    None => out.push('\\'),
                }
            } else if c == quote {
                out.push('\\');
                out.push(c);
            } else {
                out.push(c);
            }
        }
        out.push(quote);
        tok.text = out;
    }
}

fn hex_case(t: &mut [Tok], upper: bool) {
    let mut in_block = 0usize;
    let mut in_value = false;
    for tok in t.iter_mut() {
        match tok.kind {
            Kind::Open if tok.is("{") => {
                in_block += 1;
                in_value = false;
            }
            Kind::Close if tok.is("}") => {
                in_block = in_block.saturating_sub(1);
                in_value = false;
            }
            Kind::Op if tok.is(":") && in_block > 0 => in_value = true,
            Kind::Op if tok.is(";") => in_value = false,
            Kind::Word if in_value && tok.text.starts_with('#') => {
                let hex = &tok.text[1..];
                if matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
                {
                    tok.text = if upper {
                        tok.text.to_ascii_uppercase()
                    } else {
                        tok.text.to_ascii_lowercase()
                    };
                }
            }
            _ => {}
        }
    }
}

/// A token after which a JavaScript statement can end.
fn js_ends_statement(t: &Tok) -> bool {
    matches!(t.kind, Kind::Var | Kind::Num | Kind::Str | Kind::Word)
        && ![
            "return", "else", "do", "try", "finally", "const", "let", "var", "new", "typeof",
            "await", "async", "export", "import", "case", "default", "extends",
        ]
        .contains(&t.text.as_str())
        || t.is(")")
        || t.is("]")
        || t.is("++")
        || t.is("--")
}

fn js_add_semicolons(t: &mut Vec<Tok>) {
    // Open brackets: `{` of a block (true) or of an object literal (false), `(`/`[` (false).
    let mut stack: Vec<(char, bool)> = Vec::new();
    let mut i = 0;
    while i < t.len() {
        let mut closed_object = false;
        match t[i].kind {
            Kind::Open => {
                let c = t[i].text.chars().next().unwrap_or('(');
                let block = c == '{'
                    && !prev_sig(t, i).is_some_and(|p| {
                        let p = &t[p];
                        (p.kind == Kind::Op && !p.is("=>") && !p.is(")"))
                            || p.kind == Kind::Open
                            || p.is_word("return")
                    });
                stack.push((c, block));
            }
            Kind::Close => {
                if let Some((c, block)) = stack.pop() {
                    closed_object = c == '{' && !block;
                }
            }
            _ => {}
        }
        let in_block = stack.last().is_none_or(|(c, block)| *c == '{' && *block);
        let ends = (js_ends_statement(&t[i]) && t[i].kind != Kind::Close)
            || t[i].is(")")
            || t[i].is("]")
            || closed_object;
        if in_block && ends && !t[i].text.is_empty() {
            let next = next_sig(t, i).map(|n| &t[n]);
            let breaks = match next {
                None => true,
                Some(n) => {
                    (n.nl() > 0 || n.is("}"))
                        && !(n.kind == Kind::Op && !n.is("!") && !n.is("++") && !n.is("--"))
                        && !n.is(")")
                        && !n.is("]")
                        && !n.is("(")
                        && !n.is("[")
                        && !n.is("{")
                        && !n.text.starts_with('`')
                        && !n.is_word("else")
                        && !n.is_word("catch")
                        && !n.is_word("finally")
                }
            };
            // Not after the head of a block statement (`if (x)` on its own line).
            let header = t[i].is(")") && {
                let open = (0..i)
                    .rev()
                    .find(|&j| t[j].is("(") && matching(t, j) == Some(i));
                open.and_then(|o| prev_sig(t, o)).is_some_and(|p| {
                    CONTROL.contains(&t[p].text.as_str())
                        || t[p].is_word("for")
                        || t[p].is_word("function")
                })
            };
            let already = t.get(i + 1).is_some_and(|n| n.is(";"));
            if breaks && !header && !already {
                t.insert(i + 1, Tok::new(Kind::Op, ";", ""));
            }
        }
        i += 1;
    }
}

fn js_remove_semicolons(t: &mut Vec<Tok>) {
    let mut stack: Vec<char> = Vec::new();
    let mut for_header: Vec<bool> = Vec::new();
    let mut i = 0;
    while i < t.len() {
        match t[i].kind {
            Kind::Open => {
                let c = t[i].text.chars().next().unwrap_or('(');
                stack.push(c);
                for_header.push(c == '(' && prev_sig(t, i).is_some_and(|p| t[p].is_word("for")));
            }
            Kind::Close => {
                stack.pop();
                for_header.pop();
            }
            Kind::Op
                if t[i].is(";")
                    && stack.last().is_none_or(|c| *c == '{')
                    && !for_header.last().copied().unwrap_or(false) =>
            {
                let next = t.get(i + 1);
                let at_line_end = next.is_none_or(|n| n.nl() > 0 || n.text.is_empty() || n.is("}"));
                // Keep it before a line that starts with `(`, `[` or a template.
                let risky = next_sig(t, i)
                    .is_some_and(|n| t[n].is("(") || t[n].is("[") || t[n].text.starts_with('`'));
                if at_line_end && !risky {
                    let ws = std::mem::take(&mut t[i].ws);
                    t.remove(i);
                    if let Some(n) = t.get_mut(i) {
                        if n.ws.is_empty() {
                            n.ws = ws;
                        }
                    }
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
}

fn lua_is_call_target(t: &[Tok], i: usize) -> bool {
    let tok = &t[i];
    (tok.kind == Kind::Word && !LUA_KEYWORDS.contains(&tok.text.as_str())) || tok.is(")")
}

fn lua_call_parens(t: &mut Vec<Tok>, style: &str) {
    let mut i = 0;
    while i + 1 < t.len() {
        let target = lua_is_call_target(t, i);
        // `f("x")` / `f({…})` → `f "x"` / `f {…}`.
        if target && t[i + 1].is("(") && t[i + 1].ws.is_empty() {
            if let Some(close) = matching(t, i + 1) {
                let arg = &t[i + 2];
                let single_string = close == i + 3 && arg.kind == Kind::Str;
                let single_table = arg.is("{") && matching(t, i + 2) == Some(close - 1);
                let drop = (single_string && matches!(style, "noSingleString" | "none"))
                    || (single_table && matches!(style, "noSingleTable" | "none"));
                if drop && !t[i].is(")") {
                    t.remove(close);
                    t.remove(i + 1);
                    t[i + 1].ws = " ".into();
                    i += 1;
                    continue;
                }
            }
        }
        // `f "x"` / `f {…}` → `f("x")` / `f({…})`.
        if target && t[i + 1].nl() == 0 && style == "always" && t[i].kind == Kind::Word {
            let arg = &t[i + 1];
            let end = if arg.kind == Kind::Str {
                Some(i + 1)
            } else if arg.is("{") {
                matching(t, i + 1)
            } else {
                None
            };
            let is_assignment_rhs =
                prev_sig(t, i).is_some_and(|p| t[p].is_word("function") || t[p].is_word("local"));
            if let Some(end) = end.filter(|_| !is_assignment_rhs) {
                t[i + 1].ws.clear();
                t.insert(end + 1, Tok::new(Kind::Close, ")", ""));
                t.insert(i + 1, Tok::new(Kind::Open, "(", ""));
            }
        }
        i += 1;
    }
}

/// "Keep simple statements on one line": `if c then\n  return x\nend` → one line.
fn lua_collapse(t: &mut [Tok], style: &str) {
    if style == "never" {
        return;
    }
    for i in 0..t.len() {
        let opener_is_if = t[i].is_word("then");
        let opener_is_fn = t[i].is(")") && {
            let open = (0..i).rev().find(|&j| matching(t, j) == Some(i));
            open.and_then(|o| prev_sig(t, o)).is_some_and(|p| {
                t[p].is_word("function") || prev_sig(t, p).is_some_and(|q| t[q].is_word("function"))
            })
        };
        let allowed = match style {
            "functionOnly" => opener_is_fn,
            "conditionalOnly" => opener_is_if,
            _ => opener_is_fn || opener_is_if,
        };
        if !allowed || t.get(i + 1).is_none_or(|n| n.nl() == 0) {
            continue;
        }
        // The body: one line without blocks, then `end` on the next line.
        let mut j = i + 1;
        let mut ok = true;
        while j < t.len() && !(j > i + 1 && t[j].nl() > 0) {
            let w = &t[j];
            if w.kind == Kind::Comment
                || [
                    "if", "for", "while", "function", "do", "repeat", "local", "elseif", "else",
                ]
                .iter()
                .any(|k| w.is_word(k))
            {
                ok = false;
            }
            j += 1;
        }
        if ok && j < t.len() && t[j].is_word("end") && j > i + 1 {
            t[i + 1].ws = " ".into();
            t[j].ws = " ".into();
        }
    }
}

// ---- Layout ------------------------------------------------------------------------------

impl F<'_> {
    fn top(&self) -> &Frame {
        self.frames.last().expect("root frame")
    }
    fn top_mut(&mut self) -> &mut Frame {
        self.frames.last_mut().expect("root frame")
    }
    fn prev_tok(&self) -> Option<&Tok> {
        self.prev.map(|p| &self.t[p])
    }

    fn lua_closer(&self, i: usize) -> bool {
        self.lang == Lang::Lua
            && ["end", "until", "else", "elseif"]
                .iter()
                .any(|k| self.t[i].is_word(k))
    }

    fn lua_opener(&self, i: usize) -> bool {
        self.lang == Lang::Lua
            && (["then", "do", "repeat", "else"]
                .iter()
                .any(|k| self.t[i].is_word(k))
                || (self.t[i].is_word("function")))
    }

    /// Whether the statement starting at `i` declares a function (PHP and JS members).
    fn is_method(&self, i: usize) -> bool {
        let mut depth = 0usize;
        for j in i..self.t.len() {
            let t = &self.t[j];
            match t.kind {
                Kind::Open => {
                    if depth == 0 && t.is("{") {
                        return false;
                    }
                    depth += 1;
                }
                Kind::Close => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                }
                Kind::Op if depth == 0 && t.is(";") => return false,
                Kind::Word if depth == 0 && t.is_word("function") => return true,
                _ => {}
            }
            if self.lang.is_js() && depth == 0 && t.is("(") {
                return true;
            }
        }
        false
    }

    /// CSS: the statement starting at `i` is a selector (`a, b {`).
    fn is_selector(&self, i: usize) -> bool {
        for j in i..self.t.len() {
            let t = &self.t[j];
            if t.is("{") {
                return true;
            }
            if t.is(";") || t.is("}") {
                return false;
            }
        }
        false
    }

    fn classify_open(&self, i: usize) -> Fk {
        let t = &self.t[i];
        let p = self.prev.map(|p| (p, &self.t[p]));
        let pp = |p: usize| prev_sig(&self.t, p).map(|q| &self.t[q]);
        match t.text.as_str() {
            "#[" => Fk::Attr,
            "(" => {
                let Some((pi, p)) = p else {
                    return Fk::Group;
                };
                match p.kind {
                    Kind::Word if self.lang == Lang::Php && self.is_name(pi) => {
                        if prev_sig(&self.t, pi)
                            .is_some_and(|q| self.t[q].is_word("function") || self.t[q].is("&"))
                        {
                            Fk::Decl
                        } else {
                            Fk::Call
                        }
                    }
                    Kind::Word => {
                        let w = p.text.to_ascii_lowercase();
                        match w.as_str() {
                            "for" | "foreach" => Fk::For,
                            "declare" => Fk::Declare,
                            "function" | "fn" => Fk::Decl,
                            "array" | "list" if self.lang == Lang::Php => Fk::Array,
                            "use" if self.lang == Lang::Php => Fk::Decl,
                            _ if CONTROL.contains(&w.as_str()) => Fk::Control,
                            _ if PHP_NOT_OPERAND.contains(&w.as_str())
                                && !["static", "default", "delete", "void"]
                                    .contains(&w.as_str()) =>
                            {
                                Fk::Group
                            }
                            _ => {
                                let decl = pp(pi).is_some_and(|q| {
                                    q.is_word("function")
                                        || (q.is("&")
                                            && prev_sig(&self.t, pi - 1)
                                                .is_some_and(|r| self.t[r].is_word("function")))
                                }) || (self.lang.is_js()
                                    && self.top().kind == Fk::Class
                                    && self.member_header(pi))
                                    || (self.lang == Lang::Lua && self.lua_function_name(pi));
                                if decl {
                                    Fk::Decl
                                } else {
                                    Fk::Call
                                }
                            }
                        }
                    }
                    Kind::Var | Kind::Close | Kind::Str => Fk::Call,
                    _ => Fk::Group,
                }
            }
            "[" => match p {
                Some((_, p)) if is_operand(p) && !p.is_word("as") => Fk::Index,
                _ => Fk::Array,
            },
            "{" => self.classify_brace(i),
            _ => Fk::Group,
        }
    }

    /// JS class members: only modifiers before the name (`static async name(`).
    fn member_header(&self, name: usize) -> bool {
        let mut j = name;
        while let Some(p) = prev_sig(&self.t, j) {
            let t = &self.t[p];
            if t.is("{") || t.is("}") || t.is(";") {
                return true;
            }
            if ![
                "static",
                "async",
                "get",
                "set",
                "public",
                "private",
                "protected",
                "readonly",
                "*",
                "override",
            ]
            .iter()
            .any(|m| t.text == *m)
            {
                return false;
            }
            j = p;
        }
        true
    }

    /// Lua: `function a.b:c(` names a function.
    fn lua_function_name(&self, name: usize) -> bool {
        let mut j = name;
        while let Some(p) = prev_sig(&self.t, j) {
            let t = &self.t[p];
            if t.is_word("function") {
                return true;
            }
            if !(t.kind == Kind::Word || t.is(".") || t.is(":")) {
                return false;
            }
            j = p;
        }
        false
    }

    fn classify_brace(&self, i: usize) -> Fk {
        match self.lang {
            Lang::Json => return Fk::Object,
            Lang::Css => return Fk::Rule,
            Lang::Lua => return Fk::Object,
            _ => {}
        }
        let Some(pi) = self.prev else {
            return Fk::Block;
        };
        let p = &self.t[pi];
        if p.is_op("->")
            || p.is_op("?->")
            || p.is_op("::")
            || p.is("$")
            || (self.lang == Lang::Php && p.text.ends_with('\\'))
        {
            return Fk::Interp;
        }
        if self.lang.is_js()
            && (p.kind == Kind::Op && !p.is("=>") && !p.is(")")
                || p.kind == Kind::Open
                || p.is_word("return")
                || p.is_word("import")
                || p.is_word("export")
                || p.is_word("case"))
        {
            return Fk::Object;
        }
        if self.lang.is_js() && p.is("=>") {
            return Fk::Closure;
        }
        // The words of the statement header.
        let start = self.stmt_start(i);
        let words: Vec<&Tok> = self.t[start..i]
            .iter()
            .filter(|t| t.kind == Kind::Word)
            .collect();
        let has = |w: &str| words.iter().any(|t| t.is_word(w));
        if has("switch") {
            return Fk::Switch;
        }
        if has("match") && self.lang == Lang::Php {
            return Fk::Match;
        }
        if ["class", "interface", "trait", "enum"]
            .iter()
            .any(|w| has(w))
            && !has("function")
        {
            return if has("new") { Fk::Closure } else { Fk::Class };
        }
        if has("function") || has("fn") {
            let named = self.t[start..i].iter().enumerate().any(|(k, t)| {
                t.is_word("function") && {
                    let rest = &self.t[start + k + 1..i];
                    rest.iter()
                        .find(|t| !t.is("&") && !t.is("*"))
                        .is_some_and(|n| n.kind == Kind::Word)
                }
            });
            return if named { Fk::Function } else { Fk::Closure };
        }
        if self.lang.is_js() && self.top().kind == Fk::Class {
            return Fk::Function;
        }
        Fk::Block
    }

    /// First token of the statement that contains `i`.
    fn stmt_start(&self, i: usize) -> usize {
        let mut depth = 0usize;
        let mut j = i;
        while j > 0 {
            let t = &self.t[j - 1];
            if depth == 0 && t.is("}") {
                return j;
            }
            match t.kind {
                Kind::Close => depth += 1,
                Kind::Open => {
                    if depth == 0 {
                        return j;
                    }
                    depth -= 1;
                }
                Kind::Op if depth == 0 && (t.is(";") || t.is(",") && self.lang == Lang::Css) => {
                    return j
                }
                Kind::Tag => return j,
                _ => {}
            }
            j -= 1;
        }
        0
    }

    /// The brace style option for a block brace.
    fn wants_next_line(&self, kind: Fk) -> Option<bool> {
        let key = match (self.lang, kind) {
            (Lang::Php, Fk::Class) => "wrapping.classBraces",
            (Lang::Php, Fk::Function) => "wrapping.functionBraces",
            (Lang::Php, Fk::Block | Fk::Switch | Fk::Match) => "wrapping.otherBraces",
            (Lang::Js | Lang::Ts, Fk::Class | Fk::Function) => "wrapping.functionBraces",
            (Lang::Js | Lang::Ts, Fk::Block | Fk::Switch) => "wrapping.otherBraces",
            (Lang::Css, Fk::Rule) => "wrapping.braces",
            _ => return None,
        };
        Some(self.o.is(key, "nextLine"))
    }

    /// A forced line change before token `i`: `Some(true)` = new line, `Some(false)` = join.
    fn line_override(&self, i: usize) -> Option<bool> {
        let t = &self.t[i];
        let last_is_comment = self.last.is_some_and(|l| self.t[l].kind == Kind::Comment);
        let prev = self.prev_tok()?;
        if t.is("{") && self.lang != Lang::Lua {
            let kind = self.classify_brace(i);
            if let Some(next) = self.wants_next_line(kind) {
                let empty_body = self.t.get(i + 1).is_some_and(|n| n.is("}") && n.nl() == 0);
                let next = next
                    && !(self.stmt_wrapped_paren && prev.is(")"))
                    && !(empty_body && t.nl() == 0);
                if next && t.nl() == 0 {
                    return Some(true);
                }
                if !next
                    && t.nl() > 0
                    && !last_is_comment
                    && !prev.is("{")
                    && !prev.is(";")
                    && !prev.is("}")
                {
                    return Some(false);
                }
            }
            if self.mode == Mode::Layout && kind == Fk::Closure && t.nl() > 0 && !last_is_comment {
                return Some(false);
            }
        }
        if self.lang == Lang::Php || self.lang.is_js() {
            let key = if t.is_word("else") || t.is_word("elseif") {
                Some("wrapping.elseOnNewLine")
            } else if t.is_word("catch") {
                Some("wrapping.catchOnNewLine")
            } else if t.is_word("finally") {
                Some("wrapping.finallyOnNewLine")
            } else {
                None
            };
            if let Some(key) = key {
                let closed_block = self
                    .prev
                    .and_then(|p| self.closed[p])
                    .is_some_and(|(k, _)| k == Fk::Block);
                if prev.is("}") && closed_block {
                    let want = self.lang == Lang::Php && self.o.bool(key);
                    if want && t.nl() == 0 {
                        return Some(true);
                    }
                    if !want && t.nl() > 0 && !last_is_comment {
                        return Some(false);
                    }
                }
            }
        }
        if self.mode != Mode::Layout {
            return None;
        }
        match self.lang {
            Lang::Json => {
                let frame = self.top().kind;
                let key = if frame == Fk::Object {
                    "wrapping.objects"
                } else {
                    "wrapping.arrays"
                };
                let always = self.o.is(key, "always");
                let inside = matches!(frame, Fk::Object | Fk::Array);
                let empty_pair = prev.kind == Kind::Open && t.kind == Kind::Close;
                if always
                    && inside
                    && !empty_pair
                    && (prev.kind == Kind::Open || prev.is(",") || t.kind == Kind::Close)
                {
                    return Some(true);
                }
            }
            Lang::Css if prev.is(",") && self.top().kind != Fk::Group && self.is_selector(i) => {
                return Some(self.o.bool("wrapping.selectorPerLine"));
            }
            _ => {}
        }
        None
    }

    fn continuation(&self, i: usize) -> bool {
        if matches!(self.lang, Lang::Json | Lang::Css) {
            return false;
        }
        let t = &self.t[i];
        let Some(p) = self.prev_tok() else {
            return false;
        };
        if t.kind == Kind::Close || self.lua_closer(i) {
            return false;
        }
        if self.lang == Lang::Lua {
            return p.kind == Kind::Op && !p.is(",") && !p.is(";");
        }
        if self.lang.is_js() {
            let frame = self.top().kind;
            if !matches!(
                frame,
                Fk::Block | Fk::Class | Fk::Function | Fk::Closure | Fk::Switch
            ) {
                return false;
            }
            let op_continues = |x: &Tok| {
                x.kind == Kind::Op
                    && !x.is(";")
                    && !x.is(",")
                    && !x.is(":")
                    && !x.is("++")
                    && !x.is("--")
                    && !x.is("!")
            };
            return op_continues(p) || (op_continues(t) && !t.is("..."));
        }
        let frame = self.top().kind;
        if !matches!(
            frame,
            Fk::Block | Fk::Class | Fk::Function | Fk::Closure | Fk::Switch
        ) {
            return false;
        }
        if p.is(";") || p.is("{") || p.is("}") || p.is(",") || p.is(":") || p.kind == Kind::Tag {
            return false;
        }
        if self
            .prev
            .and_then(|p| self.closed[p])
            .is_some_and(|(k, _)| k == Fk::Attr)
        {
            return false;
        }
        !(t.kind == Kind::Comment && self.at_stmt_start)
    }

    fn blank_lines(&self, i: usize, orig: usize) -> usize {
        let keep = self.o.int("blankLines.keepMax");
        let mut b = orig.min(keep);
        let t = &self.t[i];
        let Some(p) = self.prev_tok() else {
            return 0;
        };
        let attached = self.last.is_some_and(|l| {
            self.t[l].kind == Kind::Comment && self.t[l].nl() > 0 && l != self.prev.unwrap_or(0)
        }) && self.last != self.prev;
        if attached || t.kind == Kind::Close {
            return b.min(if t.kind == Kind::Close { 0 } else { keep });
        }
        if p.kind == Kind::Open {
            return 0;
        }
        match self.lang {
            Lang::Php => {
                let starts = self.at_stmt_start;
                if starts && p.is(";") && self.last_stmt_word == "namespace" {
                    b = self.o.int("blankLines.afterNamespace");
                } else if starts
                    && p.is(";")
                    && self.last_stmt_word == "use"
                    && !t.is_word("use")
                    && self.frames.len() == 1
                {
                    b = self.o.int("blankLines.afterImports");
                } else if starts && self.top().kind == Fk::Class {
                    let prev_method = self
                        .prev
                        .and_then(|p| self.closed[p])
                        .is_some_and(|(k, _)| k == Fk::Function);
                    if prev_method || self.is_method(i) {
                        b = b.max(self.o.int("blankLines.aroundMethods"));
                    }
                }
                if t.is_word("return") && !p.is(":") {
                    b = b.max(self.o.int("blankLines.beforeReturn"));
                }
            }
            Lang::Css => {
                let after_rule = self
                    .prev
                    .and_then(|p| self.closed[p])
                    .is_some_and(|(k, _)| k == Fk::Rule);
                if self.at_stmt_start && (after_rule || self.is_selector(i)) {
                    b = b.max(self.o.int("blankLines.aroundRule"));
                }
            }
            _ => {}
        }
        b
    }

    // ---- Spaces ----------------------------------------------------------------------

    /// Space between the previous token and token `i` on one line: `None` keeps the input.
    fn space(&self, i: usize) -> Option<bool> {
        let t = &self.t[i];
        let pi = self.last?;
        let p = &self.t[pi];
        if p.kind == Kind::Comment || t.kind == Kind::Comment || t.text.is_empty() {
            return None;
        }
        if p.kind == Kind::Tag
            || t.kind == Kind::Tag
            || t.kind == Kind::Verbatim
            || p.kind == Kind::Verbatim
        {
            return None;
        }
        if p.is("}") && t.kind == Kind::Word && matches!(self.lang, Lang::Php | Lang::Js | Lang::Ts)
        {
            return Some(true);
        }
        match self.lang {
            Lang::Php => self.space_php(i, p, t),
            Lang::Js | Lang::Ts => self.space_js(i, p, t),
            Lang::Css => self.space_css(i, p, t),
            Lang::Json => self.space_json(p, t),
            Lang::Lua => self.space_lua(i, p, t),
            Lang::Sql => None,
        }
    }

    /// Spaces inside brackets; `None` when neither token is a bracket.
    fn space_brackets(&self, p: &Tok, t: &Tok) -> Option<Option<bool>> {
        let pi = self.last?;
        if p.kind == Kind::Open && t.kind == Kind::Close {
            return Some(Some(false));
        }
        let kind = if p.kind == Kind::Open {
            self.open_kind[pi]
        } else if t.kind == Kind::Close {
            Some(self.top().kind)
        } else {
            return None;
        };
        let kind = kind?;
        let within = match kind {
            Fk::Call | Fk::Decl | Fk::Control | Fk::For | Fk::Group => {
                self.lang != Lang::Css && self.o.bool("spaces.withinParentheses")
            }
            Fk::Declare | Fk::Attr | Fk::Index => false,
            Fk::Array => match self.lang {
                Lang::Js | Lang::Ts | Lang::Json => self.o.bool("spaces.withinBrackets"),
                Lang::Php if p.is("(") || t.is(")") => self.o.bool("spaces.withinParentheses"),
                _ => false,
            },
            Fk::Object => match self.lang {
                Lang::Js | Lang::Ts => self.o.bool("spaces.withinObjectLiteralBraces"),
                Lang::Json => self.o.bool("spaces.withinBraces"),
                Lang::Lua => self.o.bool("spaces.withinTableBraces"),
                _ => return Some(None),
            },
            Fk::Interp => false,
            _ if p.is("{") || t.is("}") => true,
            _ => return Some(None),
        };
        Some(Some(within))
    }

    /// The word at `i` names a member or a declared function (`->match(`, `function switch(`).
    fn is_name(&self, i: usize) -> bool {
        prev_sig(&self.t, i).is_some_and(|q| {
            let t = &self.t[q];
            t.is_op("->")
                || t.is_op("?->")
                || t.is_op("::")
                || t.is_word("function")
                || t.is_word("fn")
                || (t.is("&")
                    && prev_sig(&self.t, q).is_some_and(|r| self.t[r].is_word("function")))
        })
    }

    fn before_paren_php(&self, p: &Tok) -> Option<bool> {
        let o = self.o;
        let pi = self.last?;
        if p.kind == Kind::Word && self.is_name(pi) {
            let decl = prev_sig(&self.t, pi).is_some_and(|q| {
                self.t[q].is_word("function")
                    || (self.t[q].is("&")
                        && prev_sig(&self.t, q).is_some_and(|r| self.t[r].is_word("function")))
            });
            return Some(if decl {
                o.bool("spaces.beforeFunctionDeclarationParentheses")
            } else {
                o.bool("spaces.beforeFunctionCallParentheses")
            });
        }
        Some(match p.kind {
            Kind::Word => {
                let w = p.text.to_ascii_lowercase();
                match w.as_str() {
                    "if" | "elseif" => o.bool("spaces.beforeIfParentheses"),
                    "for" | "foreach" => o.bool("spaces.beforeForParentheses"),
                    "while" => o.bool("spaces.beforeWhileParentheses"),
                    "switch" | "match" => o.bool("spaces.beforeSwitchParentheses"),
                    "catch" => o.bool("spaces.beforeCatchParentheses"),
                    "function" | "fn" => o.bool("spaces.beforeAnonymousFunctionParentheses"),
                    "use" => true,
                    "array" | "list" | "isset" | "empty" | "unset" | "declare" | "exit" | "die"
                    | "eval" => false,
                    _ if PHP_NOT_OPERAND.contains(&w.as_str()) && w != "static" => return None,
                    _ => {
                        let decl = prev_sig(&self.t, pi).is_some_and(|q| {
                            self.t[q].is_word("function")
                                || (self.t[q].is("&")
                                    && prev_sig(&self.t, q)
                                        .is_some_and(|r| self.t[r].is_word("function")))
                        });
                        if decl {
                            o.bool("spaces.beforeFunctionDeclarationParentheses")
                        } else {
                            o.bool("spaces.beforeFunctionCallParentheses")
                        }
                    }
                }
            }
            Kind::Var | Kind::Close | Kind::Str => o.bool("spaces.beforeFunctionCallParentheses"),
            Kind::Op if p.is("&") => {
                // `function &name(` / `fn&(`.
                return None;
            }
            _ => return None,
        })
    }

    fn space_php(&self, i: usize, p: &Tok, t: &Tok) -> Option<bool> {
        let o = self.o;
        let pi = self.last?;
        // An attribute (`#[A] ?int $x`) is not an operand.
        let operand_before = |idx: usize| {
            prev_sig(&self.t, idx).is_some_and(|q| {
                is_operand(&self.t[q]) && !self.closed[q].is_some_and(|(k, _)| k == Fk::Attr)
            })
        };
        // Casts.
        if p.kind == Kind::Cast {
            return Some(o.bool("spaces.afterTypeCast"));
        }
        if t.is("(") && t.kind == Kind::Open {
            return self.before_paren_php(p);
        }
        if let Some(s) = self.space_brackets(p, t) {
            return s;
        }
        if t.is(",") {
            return Some(o.bool("spaces.beforeComma"));
        }
        if p.is(",") {
            return Some(o.bool("spaces.afterComma"));
        }
        if t.is(";") {
            return Some(false);
        }
        if p.is(";") {
            return Some(true);
        }
        if t.is("{") {
            return match self.classify_brace(i) {
                Fk::Interp => Some(false),
                _ => Some(true),
            };
        }
        for (a, b) in [(p, t), (t, p)] {
            if a.kind != Kind::Op {
                continue;
            }
            let at_left = std::ptr::eq(a, t);
            let s = a.text.as_str();
            if matches!(s, "->" | "?->" | "::") {
                return Some(false);
            }
            if s == "..." {
                return if at_left { None } else { Some(false) };
            }
            if s == "!" {
                return if at_left {
                    None
                } else {
                    Some(o.bool("spaces.afterUnaryNot"))
                };
            }
            if s == "@" || s == "~" {
                return if at_left { None } else { Some(false) };
            }
            if s == "++" || s == "--" {
                // `$i++` / `++$i`.
                let operand = if at_left {
                    is_operand(b)
                } else {
                    is_operand(b) && !operand_before(pi)
                };
                return if operand { Some(false) } else { None };
            }
            let key = match s {
                "=" if self.top().kind == Fk::Declare => return Some(false),
                "=" | "+=" | "-=" | "*=" | "/=" | ".=" | "%=" | "**=" | "??=" | "&=" | "|="
                | "^=" | "<<=" | ">>=" => "spaces.aroundAssignment",
                "&&" | "||" => "spaces.aroundLogical",
                "==" | "!=" | "===" | "!==" | "<>" => "spaces.aroundEquality",
                "<" | ">" | "<=" | ">=" | "<=>" => "spaces.aroundRelational",
                "*" | "/" | "%" | "**" => "spaces.aroundMultiplicative",
                "." => "spaces.aroundConcatenation",
                "??" => "spaces.aroundNullCoalescing",
                "=>" => "spaces.aroundKeyValue",
                "+" | "-" => {
                    let op_index = if at_left { i } else { pi };
                    if operand_before(op_index) {
                        "spaces.aroundAdditive"
                    } else {
                        return if at_left { None } else { Some(false) };
                    }
                }
                "?" => {
                    let op_index = if at_left { i } else { pi };
                    if !operand_before(op_index) {
                        // Nullable type.
                        return if at_left { None } else { Some(false) };
                    }
                    if !at_left && b.is(":") {
                        return Some(false);
                    }
                    return Some(true);
                }
                ":" => {
                    let op_index = if at_left { i } else { pi };
                    if self.ternary_colon[op_index] {
                        if at_left && p.is("?") {
                            return Some(false);
                        }
                        return Some(true);
                    }
                    return if at_left { Some(false) } else { Some(true) };
                }
                _ => return None,
            };
            return Some(o.bool(key));
        }
        None
    }

    fn space_js(&self, i: usize, p: &Tok, t: &Tok) -> Option<bool> {
        let o = self.o;
        let pi = self.last?;
        let operand_before = |idx: usize| {
            prev_sig(&self.t, idx)
                .map(|q| &self.t[q])
                .is_some_and(is_operand)
        };
        if t.is("(") && t.kind == Kind::Open {
            return match p.kind {
                Kind::Word => {
                    let w = p.text.as_str();
                    if ["if", "for", "while", "switch", "catch", "with"].contains(&w) {
                        Some(o.bool("spaces.beforeControlParentheses"))
                    } else if w == "function" {
                        Some(o.bool("spaces.beforeAnonymousFunctionParentheses"))
                    } else if PHP_NOT_OPERAND.contains(&w) && w != "static" || w == "async" {
                        None
                    } else if self.classify_open(i) == Fk::Decl {
                        Some(o.bool("spaces.beforeFunctionDeclarationParentheses"))
                    } else {
                        Some(false)
                    }
                }
                Kind::Close | Kind::Str | Kind::Var => Some(false),
                _ => None,
            };
        }
        if let Some(s) = self.space_brackets(p, t) {
            return s;
        }
        if t.is(",") || t.is(";") {
            return Some(false);
        }
        if p.is(",") {
            return Some(o.bool("spaces.afterComma"));
        }
        if p.is(";") {
            return Some(self.top().kind != Fk::For || o.bool("spaces.afterForSemicolon"));
        }
        if t.is("{") {
            return match self.classify_brace(i) {
                Fk::Object | Fk::Interp => None,
                _ => Some(true),
            };
        }
        for (a, b) in [(p, t), (t, p)] {
            if a.kind != Kind::Op {
                continue;
            }
            let at_left = std::ptr::eq(a, t);
            let s = a.text.as_str();
            if matches!(s, "." | "?.") {
                return Some(false);
            }
            if matches!(s, "..." | "!" | "~") {
                return if at_left { None } else { Some(false) };
            }
            if s == "++" || s == "--" {
                let operand = if at_left {
                    is_operand(b)
                } else {
                    is_operand(b) && !operand_before(pi)
                };
                return if operand { Some(false) } else { None };
            }
            let op_index = if at_left { i } else { pi };
            if (s == "<" || s == ">") && self.lang == Lang::Ts {
                return None;
            }
            if s == ":" {
                if self.ternary_colon[op_index] {
                    return Some(o.bool("spaces.aroundOperators"));
                }
                if at_left {
                    let typed = self.lang == Lang::Ts
                        && (matches!(self.top().kind, Fk::Decl)
                            || p.is(")")
                            || self.stmt_word == "let"
                            || self.stmt_word == "const"
                            || self.top().kind == Fk::Class);
                    return Some(typed && o.bool("spaces.beforeTypeColon"));
                }
                return Some(true);
            }
            if s == "?" {
                let next = next_sig(&self.t, op_index).map(|n| &self.t[n]);
                if next.is_some_and(|n| n.is(":") || n.is(")") || n.is(",") || n.is(";"))
                    || !operand_before(op_index)
                {
                    return if at_left { Some(false) } else { None };
                }
                return Some(o.bool("spaces.aroundOperators"));
            }
            if (s == "-" || s == "+" || s == "*") && !operand_before(op_index) {
                return if at_left { None } else { Some(false) };
            }
            if matches!(
                s,
                "=" | "+="
                    | "-="
                    | "*="
                    | "/="
                    | "%="
                    | "**="
                    | "<<="
                    | ">>="
                    | ">>>="
                    | "&="
                    | "|="
                    | "^="
                    | "&&="
                    | "||="
                    | "??="
                    | "=="
                    | "!="
                    | "==="
                    | "!=="
                    | "<"
                    | ">"
                    | "<="
                    | ">="
                    | "&&"
                    | "||"
                    | "??"
                    | "+"
                    | "-"
                    | "*"
                    | "/"
                    | "%"
                    | "**"
                    | "&"
                    | "|"
                    | "^"
                    | "<<"
                    | ">>"
                    | ">>>"
                    | "=>"
            ) {
                return Some(o.bool("spaces.aroundOperators"));
            }
            return None;
        }
        None
    }

    fn css_in_declaration(&self, i: usize) -> bool {
        self.top().kind == Fk::Rule && !self.is_selector(self.stmt_start(i))
    }

    fn space_css(&self, i: usize, p: &Tok, t: &Tok) -> Option<bool> {
        let o = self.o;
        if t.is("{") {
            return Some(o.bool("spaces.beforeOpeningBrace"));
        }
        if p.is("{") && t.kind != Kind::Close || t.is("}") && !p.is("{") {
            return Some(true);
        }
        if t.is(",") || t.is(";") {
            return Some(false);
        }
        if p.is(",") {
            return Some(o.bool("spaces.afterComma"));
        }
        if p.is(";") {
            return Some(true);
        }
        let declaration = self.css_in_declaration(i) || self.top().kind == Fk::Group;
        if t.is(":") && declaration {
            return Some(o.bool("spaces.beforeColon"));
        }
        if p.is(":") && declaration {
            return Some(o.bool("spaces.afterColon"));
        }
        if !declaration
            && (p.is(">") || p.is("+") || p.is("~") || t.is(">") || t.is("+") || t.is("~"))
        {
            return Some(o.bool("spaces.aroundSelectorCombinators"));
        }
        if t.is("(") {
            return if p.kind == Kind::Word
                && !p.text.starts_with('@')
                && !["and", "or", "not"].contains(&p.text.as_str())
            {
                Some(false)
            } else {
                None
            };
        }
        if p.is("(") || t.is(")") {
            return Some(false);
        }
        None
    }

    fn space_json(&self, p: &Tok, t: &Tok) -> Option<bool> {
        let o = self.o;
        if p.kind == Kind::Open && t.kind == Kind::Close {
            return Some(false);
        }
        if p.is("{") || t.is("}") {
            return Some(o.bool("spaces.withinBraces"));
        }
        if p.is("[") || t.is("]") {
            return Some(o.bool("spaces.withinBrackets"));
        }
        if t.is(":") {
            return Some(o.bool("spaces.beforeColon"));
        }
        if p.is(":") {
            return Some(o.bool("spaces.afterColon"));
        }
        if t.is(",") {
            return Some(false);
        }
        if p.is(",") {
            return Some(o.bool("spaces.afterComma"));
        }
        None
    }

    fn space_lua(&self, i: usize, p: &Tok, t: &Tok) -> Option<bool> {
        let o = self.o;
        let pi = self.last?;
        let operand_before = |idx: usize| {
            prev_sig(&self.t, idx)
                .map(|q| &self.t[q])
                .is_some_and(is_operand)
        };
        if t.is("(") && t.kind == Kind::Open {
            return match p.kind {
                Kind::Word
                    if PHP_NOT_OPERAND.contains(&p.text.as_str()) && !p.is_word("function") =>
                {
                    None
                }
                Kind::Word | Kind::Close | Kind::Str => Some(false),
                _ => None,
            };
        }
        if let Some(s) = self.space_brackets(p, t) {
            return s;
        }
        if t.is(",") || t.is(";") {
            return Some(false);
        }
        if p.is(",") {
            return Some(o.bool("spaces.afterComma"));
        }
        for (a, _b) in [(p, t), (t, p)] {
            if a.kind != Kind::Op {
                continue;
            }
            let at_left = std::ptr::eq(a, t);
            let op_index = if at_left { i } else { pi };
            let s = a.text.as_str();
            return match s {
                "." | ":" => Some(false),
                "#" => {
                    if at_left {
                        None
                    } else {
                        Some(false)
                    }
                }
                "=" => Some(o.bool("spaces.aroundAssignment")),
                ".." => Some(o.bool("spaces.aroundConcatenation")),
                "-" if !operand_before(op_index) => {
                    if at_left {
                        None
                    } else {
                        Some(false)
                    }
                }
                "==" | "~=" | "<" | ">" | "<=" | ">=" | "+" | "-" | "*" | "/" | "//" | "%"
                | "^" => Some(o.bool("spaces.aroundOperators")),
                _ => None,
            };
        }
        None
    }

    // ---- Emitting ----------------------------------------------------------------------

    fn line_indent(&self, i: usize) -> usize {
        let t = &self.t[i];
        let top = self.top();
        if t.kind == Kind::Close || self.lua_closer(i) {
            return top.base;
        }
        if t.is("{") && self.lang != Lang::Lua && !self.at_stmt_start {
            let kind = self.classify_brace(i);
            if kind.is_block() || kind == Fk::Match {
                return self.stmt_cols;
            }
        }
        let mut cols = top.inner;
        if top.kind == Fk::Switch && top.in_case && !t.is_word("case") && !t.is_word("default") {
            cols += self.ind.size;
        }
        if self.continuation(i) {
            cols += self.ind.continuation;
        }
        cols
    }

    fn emit_text(&mut self, i: usize, old_col: usize) {
        let t = &self.t[i];
        if self.mode == Mode::Layout && t.kind == Kind::Comment && t.text.contains('\n') {
            let indent = self.ind.render(self.line_cols);
            let mut lines = t.text.split('\n');
            let first = lines.next().unwrap_or("");
            let mut text = first.to_string();
            for line in lines {
                text.push('\n');
                let trimmed = line.trim_start();
                if trimmed.starts_with('*') {
                    text.push_str(&indent);
                    text.push(' ');
                    text.push_str(trimmed);
                } else if trimmed.is_empty() {
                } else {
                    let lead = line.len() - trimmed.len();
                    let drop = self.ind.width(&line[..lead]).min(old_col);
                    let keep = self.ind.width(&line[..lead]) - drop;
                    text.push_str(&indent);
                    text.push_str(&" ".repeat(keep));
                    text.push_str(trimmed);
                }
            }
            self.out.push_str(&text);
        } else {
            let text = t.text.clone();
            self.out.push_str(&text);
        }
        self.lines += self.t[i].text.matches('\n').count();
    }

    fn emit(&mut self, i: usize) {
        let first = self.last.is_none();
        let orig_nl = if first { 0 } else { self.t[i].nl() };
        let ws = self.t[i].ws.clone();
        let old_col = self.ind.width(ws.rsplit('\n').next().unwrap_or(""));
        let over = if first { None } else { self.line_override(i) };
        let new_line = match over {
            Some(n) => n,
            None => orig_nl > 0,
        };
        // Ternary bookkeeping for the spacing of `:`.
        if self.t[i].is(":") && self.top().ternary > 0 {
            self.top_mut().ternary -= 1;
            self.ternary_colon[i] = true;
        }
        if first {
            if self.mode == Mode::Spacing {
                self.out.push_str(&ws);
                self.line_ws = ws.rsplit('\n').next().unwrap_or("").to_string();
            }
        } else if new_line {
            match self.mode {
                Mode::Layout => {
                    let blank = if orig_nl > 0 {
                        self.blank_lines(i, orig_nl - 1)
                    } else {
                        0
                    };
                    let blank = if self.t[i].text.is_empty() { 0 } else { blank };
                    let cols = if self.t[i].is("{") && over == Some(true) {
                        self.stmt_cols
                    } else if over == Some(true)
                        && (self.t[i].is_word("else")
                            || self.t[i].is_word("catch")
                            || self.t[i].is_word("finally")
                            || self.t[i].is_word("elseif"))
                    {
                        self.line_cols
                    } else {
                        self.line_indent(i)
                    };
                    for _ in 0..=blank {
                        self.out.push('\n');
                    }
                    self.lines += blank + 1;
                    let indent = self.ind.render(cols);
                    self.out.push_str(&indent);
                    self.line_cols = cols;
                }
                _ => {
                    if orig_nl > 0 {
                        self.out.push_str(&ws);
                        self.lines += orig_nl;
                        self.line_ws = ws.rsplit('\n').next().unwrap_or("").to_string();
                    } else {
                        let indent = if self.t[i].is("{") {
                            self.stmt_ws.clone()
                        } else {
                            self.line_ws.clone()
                        };
                        self.out.push('\n');
                        self.out.push_str(&indent);
                        self.lines += 1;
                        self.line_ws = indent;
                    }
                }
            }
        } else {
            let space = self.space(i);
            let s = match (self.mode, space) {
                (_, Some(false)) => String::new(),
                (Mode::Layout, Some(true)) => " ".into(),
                (Mode::Layout, None) => {
                    if ws.is_empty() {
                        String::new()
                    } else {
                        " ".into()
                    }
                }
                (_, Some(true)) => {
                    if !ws.is_empty() && orig_nl == 0 {
                        ws.clone()
                    } else {
                        " ".into()
                    }
                }
                (_, None) => {
                    if orig_nl == 0 {
                        ws.clone()
                    } else {
                        " ".into()
                    }
                }
            };
            // Never glue two words.
            let s = if s.is_empty()
                && self
                    .last
                    .is_some_and(|l| word_like(&self.t[l]) && word_like(&self.t[i]))
            {
                " ".to_string()
            } else {
                s
            };
            self.out.push_str(&s);
        }
        self.emit_text(i, old_col);
        self.after(i, new_line);
    }

    /// Update the state after token `i` was written.
    fn after(&mut self, i: usize, new_line: bool) {
        let t = self.t[i].clone();
        let is_comment = t.kind == Kind::Comment;
        if !is_comment && !t.text.is_empty() && self.at_stmt_start {
            self.at_stmt_start = false;
            self.stmt_cols = self.line_cols;
            self.stmt_ws = self.line_ws.clone();
            self.stmt_word = if t.kind == Kind::Word {
                t.text.to_ascii_lowercase()
            } else {
                String::new()
            };
            self.stmt_wrapped_paren = false;
        }
        if new_line && (t.is_word("case") || t.is_word("default")) && self.top().kind == Fk::Switch
        {
            self.top_mut().in_case = true;
        }
        // Ternary `?` (PHP/JS).
        if t.is_op("?") && matches!(self.lang, Lang::Php | Lang::Js | Lang::Ts) {
            let operand = prev_sig(&self.t, i).is_some_and(|p| is_operand(&self.t[p]));
            let optional = next_sig(&self.t, i).is_some_and(|n| {
                let n = &self.t[n];
                n.is(")") || n.is(",") || n.is(";")
            });
            if operand && !optional {
                self.top_mut().ternary += 1;
            }
        }
        // Frames.
        let opens = t.kind == Kind::Open || self.lua_opener(i);
        let closes = t.kind == Kind::Close || self.lua_closer(i);
        if closes && self.frames.len() > 1 {
            // Lua `do` after `while …`/`for …` is part of the loop: nothing to close.
            let frame = self.frames.pop().expect("frame");
            let multiline = self.lines > frame.line;
            self.closed[i] = Some((frame.kind, multiline));
            if matches!(
                frame.kind,
                Fk::Call | Fk::Decl | Fk::Control | Fk::For | Fk::Group
            ) && multiline
            {
                self.stmt_wrapped_paren = true;
            }
            if frame.kind.is_block() || frame.kind == Fk::Lua || frame.kind == Fk::Match {
                self.at_stmt_start = true;
                self.last_stmt_word = std::mem::take(&mut self.stmt_word);
            }
        }
        if opens {
            let kind = if self.lang == Lang::Lua && t.kind != Kind::Open {
                Fk::Lua
            } else {
                self.classify_open(i)
            };
            let unit = if t.is("(") {
                self.ind.continuation
            } else {
                self.ind.size
            };
            let base = self.line_cols;
            self.open_kind[i] = Some(kind);
            self.frames.push(Frame {
                kind,
                base,
                inner: base + unit,
                line: self.lines,
                in_case: false,
                ternary: 0,
            });
            if kind.is_block() || kind == Fk::Lua || kind == Fk::Match {
                self.at_stmt_start = true;
                self.stmt_word.clear();
            }
        }
        if t.is(";") && self.top().kind != Fk::For {
            self.at_stmt_start = true;
            self.last_stmt_word = std::mem::take(&mut self.stmt_word);
        }
        if t.kind == Kind::Tag {
            self.at_stmt_start = true;
        }
        if !is_comment && !t.text.is_empty() {
            self.prev = Some(i);
        }
        if !t.text.is_empty() {
            self.last = Some(i);
        }
    }
}

fn word_like(t: &Tok) -> bool {
    matches!(t.kind, Kind::Word | Kind::Var | Kind::Num)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{model, Values};
    use crate::scheme::apply_preset;
    use serde_json::json;

    fn values(lang: &str, pairs: &[(&str, serde_json::Value)]) -> Values {
        let mut v = model().defaults();
        for (k, val) in pairs {
            v.insert(format!("{lang}.{k}"), val.clone());
        }
        v
    }

    fn php(src: &str, v: &Values, mode: Mode) -> String {
        format(src, Lang::Php, &Opts::new("php", v), mode)
    }

    #[test]
    fn lays_out_php_with_psr12() {
        let src = "<?php\nnamespace App;\nuse A;\nclass X extends Y {\npublic function f($a,$b) {\nif($a==$b){\nreturn $a.$b;\n}else{\nreturn (int)$a;\n}\n}\n}\n";
        let v = model().defaults();
        let out = php(src, &v, Mode::Layout);
        assert_eq!(
            out,
            "<?php\nnamespace App;\n\nuse A;\n\nclass X extends Y\n{\n    public function f($a, $b)\n    {\n        if ($a == $b) {\n            return $a . $b;\n        } else {\n            return (int) $a;\n        }\n    }\n}\n"
        );
        // Formatting again changes nothing.
        assert_eq!(php(&out, &v, Mode::Layout), out);
    }

    #[test]
    fn lays_out_php_with_wordpress() {
        let src = "<?php\nfunction f($a) {\nif (!$a) {\nreturn [1, 2];\n}\nreturn $a . 'x';\n}\n";
        let v = apply_preset(&model().defaults(), "php", "wordpress").unwrap();
        let out = php(src, &v, Mode::Layout);
        assert_eq!(
            out,
            "<?php\nfunction f( $a ) {\n\tif ( ! $a ) {\n\t\treturn array( 1, 2 );\n\t}\n\treturn $a . 'x';\n}\n"
        );
    }

    #[test]
    fn spacing_mode_keeps_lines_and_alignment() {
        let src = "<?php\n$a = [\n    'x'   => 1,\n    'yy'  => TRUE\n];\n$s = 'a'.$b . 'c';\nif ($x) {\n    f();\n}\nelse {\n    g();\n}\n";
        let v = values(
            "php",
            &[
                ("spaces.aroundConcatenation", json!(false)),
                ("codeConversion.trailingCommaInMultilineArrays", json!(true)),
            ],
        );
        let out = php(src, &v, Mode::Spacing);
        assert_eq!(
            out,
            "<?php\n$a = [\n    'x'   => 1,\n    'yy'  => true,\n];\n$s = 'a'.$b.'c';\nif ($x) {\n    f();\n} else {\n    g();\n}\n"
        );
        // Unchanged input stays byte for byte.
        let tidy = "<?php\n\nfunction f(int $a): ?string\n{\n    return $a ? 'x' : null;\n}\n";
        assert_eq!(php(tidy, &model().defaults(), Mode::Spacing), tidy);
    }

    #[test]
    fn spacing_mode_moves_braces() {
        let v = values(
            "php",
            &[
                ("wrapping.functionBraces", json!("endOfLine")),
                ("wrapping.otherBraces", json!("nextLine")),
            ],
        );
        let src = "<?php\nclass A\n{\n    public function f()\n    {\n        if ($a) {\n            g();\n        }\n    }\n}\n";
        assert_eq!(
            php(src, &v, Mode::Spacing),
            "<?php\nclass A\n{\n    public function f() {\n        if ($a)\n        {\n            g();\n        }\n    }\n}\n"
        );
        // A multi-line parameter list keeps `) {` together (PSR-12).
        let src = "<?php\nfunction f(\n    $a,\n) {\n}\n";
        assert_eq!(php(src, &model().defaults(), Mode::Spacing), src);
    }

    #[test]
    fn keyword_names_and_attributes() {
        let v = model().defaults();
        let src = "<?php\nclass A\n{\n    public function match(string $v) {}\n    public function function(#[Attr(from: '8.0')] ?string $a = null) {}\n}\n$x->switch(1);\n";
        assert_eq!(php(src, &v, Mode::Spacing), src);
    }

    #[test]
    fn converts_php_arrays() {
        let v = values("php", &[("codeConversion.arraySyntax", json!("short"))]);
        assert_eq!(
            php(
                "<?php\n$a = array(1, array('x' => 2));\n$b = $c[0];\n",
                &v,
                Mode::Spacing
            ),
            "<?php\n$a = [1, ['x' => 2]];\n$b = $c[0];\n"
        );
        let v = values("php", &[("codeConversion.arraySyntax", json!("long"))]);
        assert_eq!(
            php("<?php\n$a = [1, [2]];\n[$x, $y] = $a;\nforeach ($a as [$p]) {}\n$b = $c[0];\n", &v, Mode::Spacing),
            "<?php\n$a = array(1, array(2));\n[$x, $y] = $a;\nforeach ($a as [$p]) {}\n$b = $c[0];\n"
        );
    }

    #[test]
    fn js_layout_and_punctuation() {
        let src = "function f(a,b){\nif(a){\nreturn {x:1}\n}\nconst s = \"x\"\n}\n";
        let v = apply_preset(&model().defaults(), "javascript", "standard").unwrap();
        let out = format(src, Lang::Js, &Opts::new("javascript", &v), Mode::Layout);
        assert_eq!(
            out,
            "function f (a, b) {\n  if (a) {\n    return { x: 1 }\n  }\n  const s = 'x'\n}\n"
        );
        let v = apply_preset(&model().defaults(), "javascript", "prettier").unwrap();
        let out = format(src, Lang::Js, &Opts::new("javascript", &v), Mode::Layout);
        assert_eq!(
            out,
            "function f(a, b) {\n  if (a) {\n    return { x: 1 };\n  }\n  const s = \"x\";\n}\n"
        );
    }

    #[test]
    fn css_and_json() {
        let v = values(
            "css",
            &[
                ("other.hexColorCase", json!("lower")),
                ("wrapping.braces", json!("nextLine")),
            ],
        );
        let out = format(
            "a,b>c{\ncolor:#FFF;margin:0\n}\nd{}\n",
            Lang::Css,
            &Opts::new("css", &v),
            Mode::Layout,
        );
        assert_eq!(out, "a,\nb > c\n{\n    color: #fff; margin: 0\n}\n\nd {}\n");
        let v = model().defaults();
        let out = format(
            "{\"a\":[1,2],\"b\":{}}",
            Lang::Json,
            &Opts::new("json", &v),
            Mode::Layout,
        );
        assert_eq!(out, "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": {}\n}\n");
    }

    #[test]
    fn lua_blocks() {
        let src = "local function f(a,b)\nif a==b then\nreturn a..b\nend\nreturn print('x')\nend\n";
        let v = model().defaults();
        let out = format(src, Lang::Lua, &Opts::new("lua", &v), Mode::Layout);
        assert_eq!(out, "local function f(a, b)\n    if a == b then\n        return a .. b\n    end\n    return print(\"x\")\nend\n");
        let v = values(
            "lua",
            &[
                ("wrapping.collapseSimpleStatements", json!("always")),
                ("other.callParentheses", json!("noSingleString")),
            ],
        );
        let out = format(src, Lang::Lua, &Opts::new("lua", &v), Mode::Layout);
        assert_eq!(out, "local function f(a, b)\n    if a == b then return a .. b end\n    return print \"x\"\nend\n");
    }
}
