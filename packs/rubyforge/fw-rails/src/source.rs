//! A forgiving Ruby scanner for the static analysis: positions (bytes ↔ UTF-16 lines and
//! columns), comments, logical lines (calls continued over several lines), tokens and the
//! arguments of calls like `resources :articles, only: %i[index show]`.

/// Byte offsets of line starts, to turn byte offsets into 0-based lines and UTF-16 columns
/// (the editor's coordinates) and back.
#[derive(Debug, Clone)]
pub struct LineIndex<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex { text, starts }
    }

    /// (line, UTF-16 column) of a byte offset.
    pub fn position(&self, byte: usize) -> (u32, u32) {
        let byte = byte.min(self.text.len());
        let line = self.starts.partition_point(|&s| s <= byte) - 1;
        let start = self.starts[line];
        let col = utf16_len(self.text.get(start..byte).unwrap_or(""));
        (line as u32, col)
    }

    /// The byte offset of a 0-based line's start.
    pub fn line_start(&self, line: usize) -> usize {
        self.starts.get(line).copied().unwrap_or(self.text.len())
    }

    pub fn line_count(&self) -> usize {
        self.starts.len()
    }
}

pub fn utf16_len(s: &str) -> u32 {
    s.chars().map(|c| c.len_utf16() as u32).sum()
}

/// The byte offset of a UTF-16 offset (clamped to the text, on a char boundary).
pub fn utf16_to_byte(text: &str, offset: usize) -> usize {
    let mut units = 0usize;
    for (i, c) in text.char_indices() {
        if units >= offset {
            return i;
        }
        units += c.len_utf16();
    }
    text.len()
}

/// The UTF-16 offset of a byte offset.
pub fn byte_to_utf16(text: &str, byte: usize) -> u32 {
    let mut b = byte.min(text.len());
    while !text.is_char_boundary(b) {
        b -= 1;
    }
    utf16_len(&text[..b])
}

pub fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The byte index where a line's `#` comment starts (outside strings), or its length.
pub fn comment_start(line: &str) -> usize {
    let b = line.as_bytes();
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match quote {
            Some(q) => {
                if c == b'\\' {
                    i += 1;
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                b'"' | b'\'' | b'`' => quote = Some(c),
                b'#' => {
                    // `?#` character literals are rare enough to ignore.
                    return i;
                }
                _ => {}
            },
        }
        i += 1;
    }
    b.len()
}

/// A line without its comment, trimmed at the end.
pub fn code(line: &str) -> &str {
    line[..comment_start(line)].trim_end()
}

/// One logical line: a statement that may continue over physical lines (a trailing `,`,
/// `\`, an operator or open brackets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logical {
    /// The 0-based physical line it starts on.
    pub line: usize,
    /// Byte offset of its first character in the file.
    pub start: usize,
    /// The code (comments removed, continued lines joined with a space).
    pub text: String,
    /// Indentation of the first line.
    pub indent: usize,
}

fn open_brackets(code: &str) -> i32 {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    let mut prev = ' ';
    for c in code.chars() {
        match quote {
            Some(q) => {
                if c == q && prev != '\\' {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            },
        }
        prev = c;
    }
    depth
}

/// Split a file into logical lines (=begin/=end blocks and heredoc bodies are skipped).
pub fn logical_lines(text: &str) -> Vec<Logical> {
    let mut out: Vec<Logical> = Vec::new();
    let mut pending: Option<(Logical, i32)> = None;
    let mut in_doc = false;
    let mut heredoc: Option<String> = None;
    let mut offset = 0;
    for (n, raw) in text.split('\n').enumerate() {
        let start = offset;
        offset += raw.len() + 1;
        let raw = raw.trim_end_matches('\r');
        if in_doc {
            if raw.starts_with("=end") {
                in_doc = false;
            }
            continue;
        }
        if let Some(end) = &heredoc {
            if raw.trim() == end {
                heredoc = None;
            }
            continue;
        }
        if raw.starts_with("=begin") {
            in_doc = true;
            continue;
        }
        if raw.trim() == "__END__" {
            break;
        }
        let c = code(raw);
        if let Some(i) = c.find("<<~").or_else(|| c.find("<<-")) {
            let name: String = c[i + 3..]
                .trim_start_matches(['"', '\''])
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect();
            if !name.is_empty() && name.chars().all(|ch| ch.is_ascii_uppercase() || ch == '_') {
                heredoc = Some(name);
            }
        }
        let trimmed = c.trim();
        if let Some((mut l, depth)) = pending.take() {
            l.text.push(' ');
            l.text.push_str(trimmed);
            let depth = depth + open_brackets(trimmed);
            if continues(trimmed, depth) {
                pending = Some((l, depth));
            } else {
                out.push(l);
            }
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        let indent = c.len() - c.trim_start().len();
        let l = Logical {
            line: n,
            start: start + indent,
            text: trimmed.to_string(),
            indent,
        };
        let depth = open_brackets(trimmed);
        if continues(trimmed, depth) {
            pending = Some((l, depth));
        } else {
            out.push(l);
        }
    }
    if let Some((l, _)) = pending {
        out.push(l);
    }
    out
}

fn continues(code: &str, depth: i32) -> bool {
    depth > 0
        || code.ends_with(',')
        || code.ends_with('\\')
        || code.ends_with("=>")
        || code.ends_with("&&")
        || code.ends_with("||")
}

// ---- Tokens --------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// `resources`, `do`, `end`, `t`.
    Ident(String),
    /// `Article`, `Admin::User`.
    Const(String),
    /// `:articles`, `:"x-y"`.
    Sym(String),
    /// A string literal (`interp`: it has `#{…}`).
    Str {
        value: String,
        interp: bool,
    },
    /// `only:` / `"only":` hash keys.
    Key(String),
    /// `%w[a b]` / `%i[a b]` word lists.
    Words(Vec<String>),
    /// `@article`.
    Ivar(String),
    /// `=>`.
    Arrow,
    /// `->` (a lambda).
    Lambda,
    Num(String),
    Punct(char),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    /// Byte range in the scanned text.
    pub start: usize,
    pub end: usize,
}

impl Token {
    pub fn ident(&self) -> Option<&str> {
        match &self.tok {
            Tok::Ident(s) => Some(s),
            _ => None,
        }
    }
    pub fn is_ident(&self, name: &str) -> bool {
        self.ident() == Some(name)
    }
    pub fn is_punct(&self, c: char) -> bool {
        self.tok == Tok::Punct(c)
    }
}

fn closing(open: char) -> char {
    match open {
        '[' => ']',
        '(' => ')',
        '{' => '}',
        '<' => '>',
        c => c,
    }
}

/// Scan Ruby code into tokens (comments and unknown characters are skipped).
pub fn tokens(code: &str) -> Vec<Token> {
    let chars: Vec<(usize, char)> = code.char_indices().collect();
    let at = |i: usize| chars.get(i).map(|x| x.1).unwrap_or('\0');
    let byte = |i: usize| chars.get(i).map(|x| x.0).unwrap_or(code.len());
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = at(i);
        let start = byte(i);
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '#' {
            break;
        }
        // Strings.
        if c == '"' || c == '\'' || c == '`' {
            let (value, interp, next) = string_at(&chars, i + 1, c, c == '"');
            i = next;
            // `"key": value`
            if at(i) == ':' && at(i + 1) != ':' && c != '`' {
                out.push(Token {
                    tok: Tok::Key(value),
                    start,
                    end: byte(i + 1),
                });
                i += 1;
                continue;
            }
            out.push(Token {
                tok: Tok::Str { value, interp },
                start,
                end: byte(i),
            });
            continue;
        }
        // %w[] %i[] %q() %Q()
        if c == '%' && matches!(at(i + 1), 'w' | 'W' | 'i' | 'I' | 'q' | 'Q') {
            let open = at(i + 2);
            if matches!(open, '[' | '(' | '{' | '<' | '|' | '!') {
                let close = closing(open);
                let mut j = i + 3;
                let mut body = String::new();
                while j < chars.len() && at(j) != close {
                    body.push(at(j));
                    j += 1;
                }
                let kind = at(i + 1);
                let tok = if matches!(kind, 'q' | 'Q') {
                    Tok::Str {
                        value: body,
                        interp: false,
                    }
                } else {
                    Tok::Words(body.split_whitespace().map(str::to_string).collect())
                };
                i = (j + 1).min(chars.len());
                out.push(Token {
                    tok,
                    start,
                    end: byte(i),
                });
                continue;
            }
        }
        // Symbols.
        if c == ':' && at(i + 1) != ':' {
            let prev = if i == 0 { ' ' } else { at(i - 1) };
            let n = at(i + 1);
            if !is_ident_char(prev) && prev != ':' && (is_ident_char(n) || n == '"' || n == '\'') {
                if n == '"' || n == '\'' {
                    let (value, _, next) = string_at(&chars, i + 2, n, false);
                    i = next;
                    out.push(Token {
                        tok: Tok::Sym(value),
                        start,
                        end: byte(i),
                    });
                    continue;
                }
                let mut j = i + 1;
                while is_ident_char(at(j)) {
                    j += 1;
                }
                if matches!(at(j), '?' | '!' | '=') && at(j + 1) != '>' {
                    j += 1;
                }
                out.push(Token {
                    tok: Tok::Sym(chars[i + 1..j].iter().map(|x| x.1).collect()),
                    start,
                    end: byte(j),
                });
                i = j;
                continue;
            }
        }
        if c == '@' {
            let mut j = i + 1;
            while at(j) == '@' {
                j += 1;
            }
            let s = j;
            while is_ident_char(at(j)) {
                j += 1;
            }
            out.push(Token {
                tok: Tok::Ivar(chars[s..j].iter().map(|x| x.1).collect()),
                start,
                end: byte(j),
            });
            i = j;
            continue;
        }
        if c.is_ascii_digit() {
            let mut j = i;
            while at(j).is_ascii_alphanumeric()
                || at(j) == '_'
                || (at(j) == '.' && at(j + 1).is_ascii_digit())
            {
                j += 1;
            }
            out.push(Token {
                tok: Tok::Num(chars[i..j].iter().map(|x| x.1).collect()),
                start,
                end: byte(j),
            });
            i = j;
            continue;
        }
        if is_ident_char(c) {
            let mut j = i;
            while is_ident_char(at(j)) {
                j += 1;
            }
            if c.is_uppercase() {
                // Constant paths: `Admin::User`.
                while at(j) == ':' && at(j + 1) == ':' && is_ident_char(at(j + 2)) {
                    j += 2;
                    while is_ident_char(at(j)) {
                        j += 1;
                    }
                }
                out.push(Token {
                    tok: Tok::Const(chars[i..j].iter().map(|x| x.1).collect()),
                    start,
                    end: byte(j),
                });
                i = j;
                continue;
            }
            if matches!(at(j), '?' | '!') && at(j + 1) != '=' {
                j += 1;
            }
            let name: String = chars[i..j].iter().map(|x| x.1).collect();
            if at(j) == ':' && at(j + 1) != ':' {
                out.push(Token {
                    tok: Tok::Key(name),
                    start,
                    end: byte(j + 1),
                });
                i = j + 1;
                continue;
            }
            out.push(Token {
                tok: Tok::Ident(name),
                start,
                end: byte(j),
            });
            i = j;
            continue;
        }
        if c == '=' && at(i + 1) == '>' {
            out.push(Token {
                tok: Tok::Arrow,
                start,
                end: byte(i + 2),
            });
            i += 2;
            continue;
        }
        if c == '-' && at(i + 1) == '>' {
            out.push(Token {
                tok: Tok::Lambda,
                start,
                end: byte(i + 2),
            });
            i += 2;
            continue;
        }
        out.push(Token {
            tok: Tok::Punct(c),
            start,
            end: byte(i + 1),
        });
        i += 1;
    }
    out
}

fn string_at(
    chars: &[(usize, char)],
    mut i: usize,
    quote: char,
    interp: bool,
) -> (String, bool, usize) {
    let mut value = String::new();
    let mut has_interp = false;
    while i < chars.len() {
        let c = chars[i].1;
        if c == '\\' && i + 1 < chars.len() {
            let n = chars[i + 1].1;
            value.push(match n {
                'n' if quote == '"' => '\n',
                't' if quote == '"' => '\t',
                other => other,
            });
            i += 2;
            continue;
        }
        if c == quote {
            return (value, has_interp, i + 1);
        }
        if interp && c == '#' && chars.get(i + 1).map(|x| x.1) == Some('{') {
            has_interp = true;
        }
        value.push(c);
        i += 1;
    }
    (value, has_interp, i)
}

// ---- Call arguments ------------------------------------------------------------------------

/// A literal argument value.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Str(String),
    Sym(String),
    List(Vec<Val>),
    Const(String),
    Bool(bool),
    /// `"a" => "b#c"` (a hash rocket pair given positionally).
    Pair(Box<Val>, Box<Val>),
    /// `redirect("/x")`: the target.
    Redirect(String),
    /// A call, a lambda, a variable.
    Other(String),
}

impl Val {
    /// The text of a string or symbol.
    pub fn text(&self) -> Option<&str> {
        match self {
            Val::Str(s) | Val::Sym(s) | Val::Const(s) => Some(s),
            _ => None,
        }
    }

    /// Strings and symbols of a value or list (`only: %i[index show]`).
    pub fn texts(&self) -> Vec<String> {
        match self {
            Val::List(l) => l
                .iter()
                .filter_map(|v| v.text().map(str::to_string))
                .collect(),
            v => v.text().map(|s| vec![s.to_string()]).unwrap_or_default(),
        }
    }
}

/// The arguments of a call: positional values, keyword options, and whether a `do` block
/// follows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Args {
    pub positional: Vec<Val>,
    pub options: Vec<(String, Val)>,
    pub block: bool,
}

impl Args {
    pub fn get(&self, key: &str) -> Option<&Val> {
        self.options.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    pub fn text(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Val::text)
    }
    /// A `"path" => "controller#action"` pair among the positional arguments.
    pub fn pair(&self) -> Option<(&Val, &Val)> {
        self.positional.iter().find_map(|v| match v {
            Val::Pair(a, b) => Some((a.as_ref(), b.as_ref())),
            _ => None,
        })
    }
}

fn value(toks: &[Token], i: &mut usize) -> Val {
    let Some(t) = toks.get(*i) else {
        return Val::Other(String::new());
    };
    *i += 1;
    let v = match &t.tok {
        Tok::Str { value, .. } => Val::Str(value.clone()),
        Tok::Sym(s) => Val::Sym(s.clone()),
        Tok::Words(w) => Val::List(w.iter().map(|s| Val::Str(s.clone())).collect()),
        Tok::Const(c) => Val::Const(c.clone()),
        Tok::Ident(s) if s == "true" => Val::Bool(true),
        Tok::Ident(s) if s == "false" => Val::Bool(false),
        Tok::Punct('[') => {
            let mut items = Vec::new();
            while *i < toks.len() && !toks[*i].is_punct(']') {
                if toks[*i].is_punct(',') {
                    *i += 1;
                    continue;
                }
                let before = *i;
                items.push(value(toks, i));
                if *i == before {
                    *i += 1;
                }
            }
            *i += 1;
            Val::List(items)
        }
        Tok::Ident(s) if s == "redirect" => {
            let mut depth = 0;
            let mut target = String::new();
            while let Some(n) = toks.get(*i) {
                match &n.tok {
                    Tok::Punct('(') => depth += 1,
                    Tok::Punct(')') if depth > 0 => {
                        depth -= 1;
                        if depth == 0 {
                            *i += 1;
                            break;
                        }
                    }
                    Tok::Str { value, .. } if target.is_empty() => target = value.clone(),
                    Tok::Punct(',') if depth == 0 => break,
                    _ if depth == 0 && !matches!(n.tok, Tok::Str { .. }) => break,
                    _ => {}
                }
                *i += 1;
            }
            Val::Redirect(target)
        }
        other => {
            // Skip a call's parentheses or a lambda's braces: `redirect("/x")`, `-> { … }`.
            let name = match other {
                Tok::Ident(s) => s.clone(),
                Tok::Lambda => "->".into(),
                _ => String::new(),
            };
            let mut depth = 0;
            while let Some(n) = toks.get(*i) {
                match &n.tok {
                    Tok::Punct('(' | '[' | '{') => depth += 1,
                    Tok::Punct(')' | ']' | '}') if depth > 0 => depth -= 1,
                    Tok::Punct(',' | ')' | ']') if depth == 0 => break,
                    Tok::Ident(s) if depth == 0 && s == "do" => break,
                    _ if depth == 0
                        && !matches!(n.tok, Tok::Punct('(') | Tok::Str { .. })
                        && name != "->" =>
                    {
                        break
                    }
                    _ => {}
                }
                *i += 1;
            }
            return Val::Other(name);
        }
    };
    if toks.get(*i).map(|t| &t.tok) == Some(&Tok::Arrow) {
        *i += 1;
        let rhs = value(toks, i);
        return Val::Pair(Box::new(v), Box::new(rhs));
    }
    v
}

/// The arguments of the call whose name is `toks[0]`: `get "x", to: "a#b", as: :x do`.
pub fn call_args(toks: &[Token]) -> Args {
    let mut args = Args::default();
    let mut i = 1;
    if toks
        .get(1)
        .is_some_and(|t| t.is_punct('(') && t.start == toks[0].end)
    {
        i = 2;
    }
    while i < toks.len() {
        let t = &toks[i];
        match &t.tok {
            Tok::Punct(',' | ')' | '(' | '{' | '}') => i += 1,
            Tok::Ident(s) if s == "do" => {
                args.block = true;
                break;
            }
            Tok::Key(k) => {
                i += 1;
                let v = value(toks, &mut i);
                args.options.push((k.clone(), v));
            }
            Tok::Punct('|') => break,
            _ => {
                let before = i;
                let v = value(toks, &mut i);
                if let Val::Pair(k, rhs) = &v {
                    if let Val::Sym(key) = k.as_ref() {
                        // `:to => "a#b"`: an old-style option.
                        args.options.push((key.clone(), rhs.as_ref().clone()));
                        continue;
                    }
                }
                args.positional.push(v);
                if i == before {
                    i += 1;
                }
            }
        }
    }
    if !args.block {
        args.block = toks.iter().any(|t| t.is_ident("do"));
    }
    args
}

/// A string literal in a line around a byte offset: (content start, content end, value).
pub fn string_around(line: &str, at: usize) -> Option<(usize, usize, String)> {
    for t in tokens(line) {
        if let Tok::Str {
            value,
            interp: false,
        }
        | Tok::Sym(value) = &t.tok
        {
            let quoted = matches!(t.tok, Tok::Str { .. }) || line[t.start..].starts_with(":\"");
            let (s, e) = match &t.tok {
                Tok::Sym(_) if !quoted => (t.start + 1, t.end),
                Tok::Sym(_) => (t.start + 2, t.end.saturating_sub(1)),
                _ => (t.start + 1, t.end.saturating_sub(1).max(t.start + 1)),
            };
            if s <= at && at <= e.max(s) {
                return Some((s, e.max(s), value.clone()));
            }
        }
    }
    None
}

/// The identifier around a byte offset: (start, end, text).
pub fn word_at(text: &str, at: usize) -> Option<(usize, usize, &str)> {
    let at = at.min(text.len());
    let mut s = at;
    while s > 0 {
        let c = text[..s].chars().next_back()?;
        if !is_ident_char(c) {
            break;
        }
        s -= c.len_utf8();
    }
    let mut e = at;
    while e < text.len() {
        let c = text[e..].chars().next()?;
        if !(is_ident_char(c) || c == '?' || c == '!') {
            break;
        }
        e += c.len_utf8();
    }
    (e > s).then(|| (s, e, &text[s..e]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_are_utf16() {
        let text = "a\nüé x\n𝄞y";
        let idx = LineIndex::new(text);
        assert_eq!(idx.position(0), (0, 0));
        let x = text.find('x').unwrap();
        assert_eq!(idx.position(x), (1, 3));
        let y = text.find('y').unwrap();
        assert_eq!(idx.position(y), (2, 2));
        assert_eq!(utf16_to_byte(text, byte_to_utf16(text, y) as usize), y);
    }

    #[test]
    fn logical_lines_join_continued_calls() {
        let src =
            "get \"about\",\n    to: \"pages#about\" # the page\n=begin\nx\n=end\nroot \"a#b\"\n";
        let l = logical_lines(src);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].text, "get \"about\", to: \"pages#about\"");
        assert_eq!(l[1].line, 5);
    }

    #[test]
    fn call_arguments() {
        let t = tokens("resources :articles, only: %i[index show], path: \"posts\" do |r|");
        let a = call_args(&t);
        assert_eq!(a.positional, vec![Val::Sym("articles".into())]);
        assert_eq!(a.get("only").unwrap().texts(), ["index", "show"]);
        assert_eq!(a.text("path"), Some("posts"));
        assert!(a.block);
        let a = call_args(&tokens("get \"/old\" => \"pages#home\", :as => :old"));
        let (p, to) = a.pair().unwrap();
        assert_eq!((p.text(), to.text()), (Some("/old"), Some("pages#home")));
        assert_eq!(a.text("as"), Some("old"));
        let a = call_args(&tokens(
            "match \"x\", to: redirect(\"/y\"), via: [:get, :post]",
        ));
        assert_eq!(a.get("to"), Some(&Val::Redirect("/y".into())));
        assert_eq!(a.get("via").unwrap().texts(), ["get", "post"]);
        let a = call_args(&tokens("scope(path: \"/admin\", module: :admin) do"));
        assert_eq!(a.text("module"), Some("admin"));
        assert!(a.block);
    }

    #[test]
    fn strings_and_words_around_offsets() {
        let line = "<%= t(\".title\") %> <%= render \"form\" %>";
        let at = line.find("title").unwrap() + 2;
        assert_eq!(string_around(line, at).unwrap().2, ".title");
        assert_eq!(
            word_at("link_to x, articles_path", 15).unwrap().2,
            "articles_path"
        );
        assert_eq!(code("x = 1 # \"y\""), "x = 1");
        assert_eq!(code("x = \"#{a}\" # c"), "x = \"#{a}\"");
    }
}
